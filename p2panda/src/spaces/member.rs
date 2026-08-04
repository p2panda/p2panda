// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::time::Duration;

use p2panda_core::traits::ShortFormat;
use p2panda_spaces::{ActorId, SpaceId};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, error};

use crate::operation::Operation;
use crate::spaces::Group;
use crate::spaces::types::{InnerMember, SpacesManager, SpacesManagerError};
use crate::streams::{ImportLocalTx, LocalStreamFuture};

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
pub enum MemberError {
    #[error(transparent)]
    Manager(#[from] SpacesManagerError),
}

/// Background task to automatically publish a new member message into all registered space streams if
/// the associated key bundle is about to expire.
pub struct KeyBundleTask {
    tx: mpsc::UnboundedSender<KeyBundleTaskCommand>,
    handle: JoinHandle<()>,
}

impl KeyBundleTask {
    /// Spawn key bundle background task.
    pub fn spawn(manager: SpacesManager) -> Self {
        debug!("key bundle management task started");

        let (tx, rx) = mpsc::unbounded_channel();

        let handle = tokio::spawn(async move {
            let result = renew_expired_key_bundles(manager, rx).await;

            match result {
                Ok(_) => debug!("key bundle management task ended"),
                Err(ref err) => {
                    error!("failed task to automatically renew key bundles: {}", err);
                }
            }
        });

        Self { tx, handle }
    }

    /// Use the returned sender to add and remove space streams.
    pub fn command_handle(&self) -> mpsc::UnboundedSender<KeyBundleTaskCommand> {
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
    AddStream(SpaceId, ImportLocalTx),

    /// Remove inactive / closed stream from the list.
    RemoveStream(SpaceId),
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

                debug!(
                    spaces_streams_len = spaces_streams.len(),
                    "key bundle expired, automatically generate new one"
                );

                let operation = manager.key_bundle_message().await?.into_operation();

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

#[cfg(test)]
mod tests {
    use p2panda_core::test_utils::setup_logging;
    use p2panda_store::SqliteStore;

    use crate::Credentials;
    use crate::forge::OperationForge;
    use crate::streams::TaskTracker;

    use super::KeyBundleTask;

    #[tokio::test]
    async fn inform_spaces_about_new_key_bundles() {
        setup_logging();

        let spaces_manager = {
            let store = SqliteStore::temporary().await;
            let tasks = TaskTracker::new();
            let credentials = Credentials::generate();
            let forge = OperationForge::new(credentials.clone(), store.clone());
            crate::spaces::spaces_manager(forge, credentials, store.clone()).unwrap()
        };

        let task = KeyBundleTask::spawn(spaces_manager);
    }
}
