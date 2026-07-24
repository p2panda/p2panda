// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;

use futures_util::Stream;
use futures_util::stream::{SelectAll, StreamExt};
use p2panda_net::discovery::DiscoveryEvent;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::spaces::GroupEvent;

/// System event.
///
/// System events encompass all network-related events which are not directly associated with a
/// topic.
#[derive(Clone, Debug, PartialEq)]
// @TODO: GroupEvent is a large type and I believe it will remain that way, maybe best to Box here.
#[allow(clippy::large_enum_variant)]
pub enum SystemEvent {
    Discovery(DiscoveryEvent),
    Auth(GroupEvent),
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
