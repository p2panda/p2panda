// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_net::sync::authoriser::SyncBlockList;
use p2panda_spaces::SpaceEvent;
use p2panda_stream::hooks::ProcessorHook;
use p2panda_stream::spaces::SpacesResult;

use crate::processor::ProcessorStatus;
use crate::spaces::types::AuthCapabilities;
use crate::streams::Event;

/// Pipeline hook to observe spaces events and add any members we observe being removed to the sync
/// block-list for the space topic.
///
/// NOTE: State for the authoriser is not persisted so it's important that it be populated with
/// initial state on startup if required.
//
// TODO: This should be state-less and implement something like SyncHook instead, quering the
// internal spaces state to allow/block sync sessions.
//
// See related issue: <https://github.com/p2panda/p2panda/issues/1441>.
pub struct SyncAuthoriserHook {
    inner: SyncBlockList,
}

impl SyncAuthoriserHook {
    pub fn new(inner: SyncBlockList) -> Self {
        Self { inner }
    }
}

impl ProcessorHook<Event> for SyncAuthoriserHook {
    async fn on_input(&self, input: &Event) {
        let ProcessorStatus::Completed(ref result) = input.spaces else {
            return;
        };

        let SpacesResult::Processed { events } = result else {
            return;
        };

        update_authoriser(&self.inner, events).await;
    }
}

pub(crate) async fn update_authoriser(
    sync_block_list: &SyncBlockList,
    events: impl IntoIterator<Item = &p2panda_spaces::Event<AuthCapabilities>>,
) {
    for event in events {
        let p2panda_spaces::Event::Spaces(space_event) = event else {
            continue;
        };
        let (space_id, members) = match space_event {
            SpaceEvent::Created {
                space_id, context, ..
            } => (space_id, &context.members),
            SpaceEvent::Added {
                space_id, context, ..
            } => (space_id, &context.members),
            SpaceEvent::Removed {
                space_id,
                removed,
                context,
                ..
            } => {
                // For remove events add removed members to the topic block-list.
                for (member, _) in removed {
                    sync_block_list
                        .block_topic(*member, { *space_id }.into())
                        .await;
                }

                (space_id, &context.members)
            }
            _ => return,
        };

        // For all events add current members to the topic allow-list.
        //
        // This catches the case where a previously removed member has been re-added.
        for (member, _) in members {
            sync_block_list
                .allow_topic(*member, { *space_id }.into())
                .await;
        }
    }
}
