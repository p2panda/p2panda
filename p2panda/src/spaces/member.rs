// SPDX-License-Identifier: MIT OR Apache-2.0

//! Members who are part of an encrypted space.
//!
//! They are authors who can publish application messages into a space (when they have "write"
//! access) and / or decrypt incoming ones (when they have "read" access).
//!
//! Every member is identified by its verification key (Ed25519). For initial key-agreement we
//! internally use Signal's X3DH which requires an additional identity key (xEdDSA) and a pre-key
//! (X25519).
//!
//! ## Key bundles
//!
//! Key bundles are used across all spaces to add a member or inform them about a new group secret
//! for the first time. This means that _any_ current member of a group which introduces the first
//! or a new group secret (via a CREATE, REMOVE or UPDATE) needs to have everyone else's key bundles
//! available if these two members have never exchanged any secrets yet.
//!
//! NOTE: This rule currently applies to each space separately, we are currently looking into
//! sharing this "two party" / X3DH key agreement state across all spaces, so we only need to do an
//! initial key agreement with a key bundle _once_ per pair of members across all spaces.
//!
//! See related issue: <https://github.com/p2panda/p2panda/issues/1346>
//!
//! ## Exchanging key bundles
//!
//! The pre-key is published in form of a key bundle in the member's own log. This log is dedicated
//! to only these kinds of messages.
//!
//! Alternatively they can also be shared "offchain" in a side-channel, for example via a QR code
//! etc. to increase privacy.
//!
//! ## Expiring key bundles
//!
//! Key bundles expire after a configured number of days and need to be renewed for forward secrecy.
//! All connected spaces have to be able to sync the new key bundles.
//!
//! ## Space association
//!
//! Since this log is maintained independent of a particular space we need to explicitly associate
//! it when the space starts to depend on the member's key bundles.
use std::collections::HashMap;
use std::time::Duration;

use p2panda_auth::Access;
use p2panda_core::VerifyingKey;
use p2panda_core::traits::ShortFormat;
use p2panda_net::NodeId;
use p2panda_spaces::{ActorId, MemberId, SpaceId};
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore, tx};
use p2panda_stream::hooks::ProcessorHook;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error};

use crate::operation::Operation;
use crate::spaces::Group;
use crate::spaces::forge::member_log_id;
use crate::spaces::types::{AuthCapabilities, InnerMember, SpacesManager, SpacesManagerError};
use crate::streams::{Event, ImportLocalTx, LocalStreamFuture};

const CHECK_KEY_BUNDLE_FREQUENCY: Duration = Duration::from_mins(15);

#[derive(Debug)]
pub struct Member {
    pub(crate) inner: InnerMember,
}

impl Member {
    pub fn id(&self) -> ActorId {
        self.inner.id()
    }
}

impl From<Member> for ActorId {
    fn from(value: Member) -> Self {
        value.inner.id()
    }
}

impl From<Member> for p2panda_spaces::member::Member {
    fn from(value: Member) -> p2panda_spaces::member::Member {
        value.inner
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupActor {
    pub(crate) id: ActorId,
    pub(crate) group: bool,
}

impl GroupActor {
    pub fn id(&self) -> ActorId {
        self.id
    }

    pub fn is_group(&self) -> bool {
        self.group
    }
}

impl From<p2panda_spaces::GroupActor> for GroupActor {
    fn from(actor: p2panda_spaces::GroupActor) -> Self {
        Self {
            id: actor.id(),
            group: actor.is_group(),
        }
    }
}

impl From<Member> for GroupActor {
    fn from(member: Member) -> Self {
        Self {
            id: member.id(),
            group: false,
        }
    }
}

impl From<Group> for GroupActor {
    fn from(group: Group) -> Self {
        Self {
            id: group.id(),
            group: true,
        }
    }
}

impl From<GroupActor> for ActorId {
    fn from(value: GroupActor) -> Self {
        value.id
    }
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct MemberError(#[from] SpacesManagerError);

pub type KeyBundleTaskSender = mpsc::UnboundedSender<KeyBundleTaskCommand>;

/// Background task to automatically publish a new member message into all currently active space
/// streams if the associated key bundle is about to expire.
#[derive(Clone, Debug)]
pub struct KeyBundleTask {
    tx: KeyBundleTaskSender,
}

impl KeyBundleTask {
    /// Spawn key bundle background task.
    pub fn spawn(manager: SpacesManager) -> Self {
        debug!("key bundle management task started");

        let (tx, rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            let result = renew_expired_key_bundles(manager, rx).await;

            match result {
                Ok(_) => debug!("key bundle management task ended"),
                Err(ref err) => {
                    error!("failed task to automatically renew key bundles: {}", err);
                }
            }
        });

        Self { tx }
    }

    /// Use the returned sender to add and remove active space streams.
    pub fn command_handle(&self) -> KeyBundleTaskSender {
        self.tx.clone()
    }
}

/// Command for key bundle management task.
#[derive(Debug)]
pub enum KeyBundleTaskCommand {
    /// Add a new spaces stream to list.
    ///
    /// The task will automatically publish "member" messages with the newly generated key bundle
    /// into each stream in the list when the current key bundle is about to expire.
    ///
    /// This allows currently connected nodes to directly receive these messages in "live-mode" as
    /// they get eagerly pushed towards them. Offline nodes will pick them up later as part of the
    /// regular sync protocol.
    AddStream(SpaceId, ImportLocalTx),

    /// Remove inactive / closed stream from the list.
    RemoveStream(SpaceId),
}

async fn renew_expired_key_bundles(
    manager: SpacesManager,
    mut rx: mpsc::UnboundedReceiver<KeyBundleTaskCommand>,
) -> Result<(), SpacesManagerError> {
    // Keep a list of all spaces streams where we publish the new "member" message into when a key
    // bundle is about to expire.
    //
    // TODO: Instead of this space id -> import stream association the whole thing should be it's
    // own object (something like an "local import handle"), we should be able to create one
    // directly from a stream object.
    let mut spaces_streams: HashMap<SpaceId, ImportLocalTx> = HashMap::new();

    // The interval always fires at start, later in the given frequency. This assures that we always
    // check the current key bundle at least once on process start.
    let mut interval = tokio::time::interval(CHECK_KEY_BUNDLE_FREQUENCY);
    loop {
        tokio::select! {
            biased;

            _ = interval.tick() => {
                if !manager.key_bundle_expired().await? {
                    continue;
                }

                let operation = manager.key_bundle_message().await?.into_operation();

                debug!(
                    active_streams = spaces_streams.len(),
                    seq_num = operation.header.seq_num,
                    "key bundle non-existent or expired, automatically generate new one"
                );

                let mut failed_sends = Vec::new();

                for (space_id, import_local_tx) in spaces_streams.iter() {
                    let success = publish_member_message(
                        operation.clone(),
                        space_id,
                        import_local_tx,
                    )
                    .await;

                    if !success {
                        failed_sends.push(*space_id);
                    }
                }

                // Automatically remove streams from list where sending message failed.
                for space_id in failed_sends.iter() {
                    spaces_streams.remove(space_id);
                }
            }

            command = rx.recv() => {
                let Some(command) = command else {
                    // Stop task when all senders were dropped.
                    return Ok(());
                };

                match command {
                    KeyBundleTaskCommand::AddStream(space_id, import_local_tx) => {
                        spaces_streams.insert(space_id, import_local_tx);
                    },
                    KeyBundleTaskCommand::RemoveStream(space_id) => {
                        spaces_streams.remove(&space_id);
                    }
                }
            }
        }
    }
}

async fn publish_member_message(
    operation: Operation,
    space_id: &SpaceId,
    import_local_tx: &ImportLocalTx,
) -> bool {
    let stream = Box::pin(futures_util::stream::once(async { operation }));

    // We don't need to await the result.
    let (ready_tx, _) = oneshot::channel::<LocalStreamFuture>();

    if let Err(err) = import_local_tx.send((stream, ready_tx)).await {
        debug!(
            space_id = %space_id.fmt_short(),
            "sending member message failed due to error: {err}"
        );

        return false;
    }

    true
}

/// Associate member log's by observing spaces events.
///
/// This helps with the following problem:
///
/// ```text
/// Peer A creates space S with B, C, D
///   A associates their member log to S
/// Peer B joins space S
///   B associates their member log to S
/// Peer B wants to remove C
///
/// --> Peer B needs to inform A & D about the new secret but doesn't have the member log of D yet.
/// ```
///
/// If Peer B would already associate D's member log when they've observed the CREATE event they
/// would have had a chance to receive it from A. This hook makes sure that the association takes
/// place.
pub async fn associate_members(
    my_node_id: VerifyingKey,
    store: &SqliteStore,
    events: &[p2panda_spaces::Event<AuthCapabilities>],
) -> Option<(SpaceId, Vec<MemberId>)> {
    for event in events {
        let p2panda_spaces::Event::Spaces(space_event) = event else {
            continue;
        };

        let (space_id, context) = match space_event {
            p2panda_spaces::SpaceEvent::Created {
                space_id, context, ..
            } => (space_id, context),
            p2panda_spaces::SpaceEvent::Added {
                space_id, context, ..
            } => (space_id, context),
            _ => continue,
        };

        // We have to look at _current_ members instead of only the added ones since we might not
        // process all events from the beginning, especially if we've been added later to the group.
        let members = &context.members;

        if let Err(err) = associate_members_inner(store, space_id, members).await {
            error!(
                my_node_id = %my_node_id.fmt_short(),
                space_id = %space_id.fmt_short(),
                "member association failed: {err}"
            );
        } else {
            debug!(
                my_node_id = %my_node_id.fmt_short(),
                space_id = %space_id.fmt_short(),
                "associate {} member logs", members.len()
            );
        }
    }

    None
}

async fn associate_members_inner(
    store: &SqliteStore,
    space_id: &SpaceId,
    members: &[(VerifyingKey, Access)],
) -> Result<(), SqliteError> {
    tx!(store, {
        for (id, _) in members {
            store.associate(space_id, id, &member_log_id()).await?;
        }
    });

    Ok(())
}

/// Pipeline hook to observe spaces events and automatically associate member logs to the space when
/// someone new was added.
pub struct MemberAssociationHook {
    my_node_id: NodeId,
    store: SqliteStore,
}

impl MemberAssociationHook {
    pub fn new(my_node_id: NodeId, store: SqliteStore) -> Self {
        Self { my_node_id, store }
    }
}

impl ProcessorHook<Event> for MemberAssociationHook {
    async fn on_input(&self, input: &Event) {
        let crate::processor::ProcessorStatus::Completed(ref result) = input.spaces else {
            return;
        };

        let p2panda_stream::spaces::SpacesResult::Processed { events } = result else {
            return;
        };

        associate_members(self.my_node_id, &self.store, events).await;
    }
}
