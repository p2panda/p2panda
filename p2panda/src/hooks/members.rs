// SPDX-License-Identifier: MIT OR Apache-2.0

//! Members log association via pipeline hooks.
//!
//! Logs of members contain public key bundle material we need for initial key agreement with this
//! member. This material is required when encrypting a new group key towards someone for the first
//! time in a space. This can happen when removing a member or manually rotating it ("update").
//!
//! When creating a space or adding someone the users will make sure to collect the member's key
//! bundle beforehands. However, for other members in the space we can't assume that they'll collect
//! the key bundles.
//!
//! Concretly, this is the problem we solve with automatic members log association:
//!
//! ```text
//! Node [A] creates space S with [B], [C], [D]
//!   [A] associates their member log to S
//! Node [B] joins space S
//!   [B] associates their member log to S
//! Node [B] wants to remove [C]
//!
//! --> Node [B] needs to inform [A] & [D] about the new secret but doesn't have the member log of
//! [D] yet.
//! ```
//!
//! If Node B would already associate D's member log when they've observed the CREATE event they
//! would have had a chance to receive it from A. This hook makes sure that the association takes
//! place and we have a chance to sync key bundles in the background.
//!
//! Operations of newly associated member logs are pushed to all Node in live mode to make sure key
//! bundles travel to everyone as fast as possible.
use futures_util::StreamExt;
use p2panda_auth::Access;
use p2panda_core::traits::ShortFormat;
use p2panda_core::{Topic, VerifyingKey};
use p2panda_net::NodeId;
use p2panda_spaces::SpaceId;
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore, tx};
use p2panda_stream::hooks::ProcessorHook;
use p2panda_stream::spaces::SpacesResult;
use thiserror::Error;
use tracing::{debug, error};

use crate::egress::{
    DispatchConfig, EgressHandle, EventDeliveryPolicy, EventProcessingPolicy, SubmitError,
};
use crate::processor::ProcessorStatus;
use crate::spaces::member_log_id;
use crate::spaces::types::AuthCapabilities;
use crate::streams::Event;

/// Pipeline hook to observe spaces events and automatically associate member logs to the space when
/// someone new was added.
pub struct MembersHook {
    my_node_id: NodeId,
    store: SqliteStore,
    egress_handle: EgressHandle,
}

impl MembersHook {
    pub fn new(my_node_id: NodeId, store: SqliteStore, egress_handle: EgressHandle) -> Self {
        Self {
            my_node_id,
            store,
            egress_handle,
        }
    }
}

impl ProcessorHook<Event> for MembersHook {
    async fn on_input(&self, input: &Event) {
        let ProcessorStatus::Completed(ref result) = input.spaces else {
            return;
        };

        let SpacesResult::Processed { events } = result else {
            return;
        };

        on_members_hook(self.my_node_id, &self.store, &self.egress_handle, events).await;
    }
}

/// Associate member logs by observing spaces events.
async fn on_members_hook(
    my_node_id: VerifyingKey,
    store: &SqliteStore,
    egress_handle: &EgressHandle,
    events: impl IntoIterator<Item = &p2panda_spaces::Event<AuthCapabilities>>,
) {
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

        match associate_members_inner(store, space_id, members).await {
            Ok(new_members) => {
                debug!(
                    my_node_id = %my_node_id.fmt_short(),
                    space_id = %space_id.fmt_short(),
                    "associate {} member logs, {} new", members.len(), new_members.len()
                );

                if !new_members.is_empty() {
                    let store = store.clone();
                    let egress_handle = egress_handle.clone();
                    let topic = Topic::from(*space_id);

                    tokio::spawn(async move {
                        let result =
                            dispatch_member_logs(store, egress_handle, topic, new_members).await;

                        if let Err(err) = &result {
                            error!(
                                my_node_id = %my_node_id.fmt_short(),
                                space_id = %topic.fmt_short(),
                                "dispatching member logs failed: {err}"
                            );
                        }
                    });
                }
            }
            Err(err) => {
                error!(
                    my_node_id = %my_node_id.fmt_short(),
                    space_id = %space_id.fmt_short(),
                    "member association failed: {err}"
                );
            }
        }
    }
}

async fn associate_members_inner(
    store: &SqliteStore,
    space_id: &SpaceId,
    members: &[(VerifyingKey, Access)],
) -> Result<Vec<VerifyingKey>, SqliteError> {
    let log_id = member_log_id();
    let mut new_members = Vec::new();

    tx!(store, {
        for (id, _) in members {
            if store.associate_tx(space_id, id, &log_id).await? {
                new_members.push(*id);
            }
        }
    });

    Ok(new_members)
}

/// Dispatch the full member log to all live nodes.
async fn dispatch_member_logs(
    store: SqliteStore,
    egress_handle: EgressHandle,
    topic: Topic,
    members: Vec<VerifyingKey>,
) -> Result<(), ReplayMemberLogsError> {
    let log_id = member_log_id();

    let config = DispatchConfig {
        delivery: EventDeliveryPolicy::Topic(topic),
        processing: EventProcessingPolicy::Disabled,
    };

    for member in members {
        // We need to dispatch the full log as we can't assume that everyone who receives it (live
        // mode nodes) will have all previous entries. Otherwise these operations will be rejected
        // as there's gaps in the log.
        //
        // TODO: Later we want to prune the log when key bundles expired, so we don't send a whole
        // log with outdated material inside.
        let mut entries = store.log_entries(&member, &log_id, None, None)?;

        while let Some(result) = entries.next().await {
            let operation = result?
                .entry
                .try_into()
                .expect("already validated extensions");

            egress_handle
                .dispatch_with_config(operation, &config)
                .await?;
        }
    }

    Ok(())
}

#[derive(Debug, Error)]
enum ReplayMemberLogsError {
    #[error(transparent)]
    Sqlite(#[from] SqliteError),

    #[error(transparent)]
    Submit(#[from] SubmitError),
}
