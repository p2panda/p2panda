// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_net::connection_authoriser::ConnectionAuthoriser;
use p2panda_spaces::SpaceEvent;
use p2panda_stream::hooks::ProcessorHook;
use p2panda_stream::spaces::SpacesResult;

use crate::processor::ProcessorStatus;
use crate::spaces::types::AuthCapabilities;
use crate::streams::Event;

/// Pipeline hook to observe spaces events and add any members we observe being removed to the
/// connection block-list for the space topic.
///
/// NOTE: State for the connection authoriser is not persisted so it's important that it be
/// populated with initial state on startup if required.
pub struct ConnectionAuthoriserHook {
    inner: ConnectionAuthoriser,
}

impl ConnectionAuthoriserHook {
    pub fn new(inner: ConnectionAuthoriser) -> Self {
        Self { inner }
    }
}

impl ProcessorHook<Event> for ConnectionAuthoriserHook {
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
    connection_authoriser: &ConnectionAuthoriser,
    events: &Vec<p2panda_spaces::Event<AuthCapabilities>>,
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
                    connection_authoriser
                        .topic_block(*member, { *space_id }.into())
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
            connection_authoriser
                .topic_allow(*member, { *space_id }.into())
                .await;
        }
    }
}
