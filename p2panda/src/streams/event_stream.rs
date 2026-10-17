// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;

use futures_util::Stream;
use futures_util::stream::{SelectAll, StreamExt};
use p2panda_auth::AccessLevel;
use p2panda_net::discovery::DiscoveryEvent;
use p2panda_net::sync::authoriser::SyncBlockListEvent;
use p2panda_spaces::{ActorId, GroupId};
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
    /// Allow / block events from sync sessions.
    SyncAuthoriser(SyncBlockListEvent),

    /// Discovery protocol events.
    Discovery(DiscoveryEvent),

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

pub type EventStream = Pin<Box<dyn Stream<Item = SystemEvent> + Send + Unpin + 'static>>;

/// Merge the provided event streams into a single, unified system event stream.
pub(crate) fn event_stream(
    system_events: broadcast::Receiver<SystemEvent>,
    sync_block_list_events: broadcast::Receiver<SyncBlockListEvent>,
    discovery_events: broadcast::Receiver<DiscoveryEvent>,
) -> EventStream {
    let sync_block_stream = BroadcastStream::new(sync_block_list_events);
    let sync_block_stream: Pin<Box<dyn Stream<Item = SystemEvent> + Send>> = Box::pin(
        sync_block_stream
            .filter_map(|event| async { event.ok().map(SystemEvent::SyncAuthoriser) })
            .boxed(),
    );

    let discovery_stream = BroadcastStream::new(discovery_events);
    let discovery_stream: Pin<Box<dyn Stream<Item = SystemEvent> + Send>> = Box::pin(
        discovery_stream
            .filter_map(|event| async { event.ok().map(SystemEvent::Discovery) })
            .boxed(),
    );

    let system_events_stream = BroadcastStream::new(system_events);
    let system_events_stream: Pin<Box<dyn Stream<Item = SystemEvent> + Send>> =
        Box::pin(system_events_stream.filter_map(|event| async { event.ok() }));

    let mut stream_set = SelectAll::new();
    stream_set.push(sync_block_stream);
    stream_set.push(discovery_stream);
    stream_set.push(system_events_stream);

    Box::pin(stream_set)
}
