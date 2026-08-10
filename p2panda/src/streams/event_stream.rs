// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;

use futures_util::Stream;
use futures_util::stream::{SelectAll, StreamExt};
use p2panda_auth::AccessLevel;
use p2panda_net::discovery::DiscoveryEvent;
use p2panda_spaces::{ActorId, GroupId, MemberId};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::spaces::GroupActor;
use crate::spaces::types::InnerGroupEvent;

/// System event.
///
/// System events encompass all network-related events which are not directly associated with a
/// topic.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum SystemEvent {
    /// Events of the discovery protocol.
    Discovery(DiscoveryEvent),

    /// Received member info with associated key-bundle.
    MemberInfoReceived { member_id: MemberId },

    /// Group change occurred.
    Groups {
        /// Id of the group this event originated from.
        group_id: GroupId,

        /// Current group members.
        members: Vec<(ActorId, AccessLevel)>,

        /// Current actor members (can contain individuals and groups).
        actors: Vec<(GroupActor, AccessLevel)>,

        /// Inner group event.
        ///
        /// Contains additionally meta information regarding the exact change that occurred.
        inner: InnerGroupEvent,
    },
}

/// Merge the provided event streams into a single, unified system event stream.
pub(crate) fn event_stream(
    events_stream: broadcast::Receiver<SystemEvent>,
    discovery_events: broadcast::Receiver<DiscoveryEvent>,
) -> impl Stream<Item = SystemEvent> + Send + Unpin + 'static {
    let discovery_broadcast_stream = BroadcastStream::new(discovery_events);

    let discovery_stream: Pin<Box<dyn Stream<Item = SystemEvent> + Send>> = Box::pin(
        discovery_broadcast_stream
            .filter_map(|event| async { event.ok().map(SystemEvent::Discovery) }),
    );

    let events_broadcast_stream = BroadcastStream::new(events_stream);

    let events_stream: Pin<Box<dyn Stream<Item = SystemEvent> + Send>> =
        Box::pin(events_broadcast_stream.filter_map(|event| async { event.ok() }));

    let mut stream_set = SelectAll::new();
    stream_set.push(discovery_stream);
    stream_set.push(events_stream);

    Box::pin(stream_set)
}
