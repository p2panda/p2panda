// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_auth::Access;
use p2panda_core::VerifyingKey;
use p2panda_core::traits::ShortFormat;
use p2panda_net::NodeId;
use p2panda_spaces::{MemberId, SpaceId};
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore, tx};
use p2panda_stream::hooks::ProcessorHook;
use tracing::{debug, error};

use crate::spaces::member_log_id;
use crate::spaces::types::AuthCapabilities;
use crate::streams::Event;

/// Pipeline hook to observe spaces events and automatically associate member logs to the space when
/// someone new was added.
pub struct MembersHook {
    my_node_id: NodeId,
    store: SqliteStore,
}

impl MembersHook {
    pub fn new(my_node_id: NodeId, store: SqliteStore) -> Self {
        Self { my_node_id, store }
    }
}

impl ProcessorHook<Event> for MembersHook {
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
async fn associate_members(
    my_node_id: VerifyingKey,
    store: &SqliteStore,
    events: impl IntoIterator<Item = &p2panda_spaces::Event<AuthCapabilities>>,
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
    let log_id = member_log_id();

    tx!(store, {
        for (id, _) in members {
            store.associate_tx(space_id, id, &log_id).await?;
        }
    });

    Ok(())
}
