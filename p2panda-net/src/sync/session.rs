// SPDX-License-Identifier: MIT OR Apache-2.0

//! Building the sync sessions the topic manager runs.
use std::fmt::Debug;

use p2panda_core::traits::Digest;
use p2panda_core::{Hash, Topic};
use p2panda_sync::ToSync;
use p2panda_sync::traits::Protocol;
use tokio::sync::mpsc;

/// Interface required for building LogSync sessions.
/// 
/// Note: this is only used internally, the generics are mainly there for testing purposes and to
/// keep trait requirements small in the actors.
pub trait SessionBuilder: Clone + Send + 'static {
    type Protocol: Protocol + Send + 'static;

    type Operation: Digest<Hash> + Clone + Debug + Send + 'static;

    type Event: SyncSessionItem<Self::Operation> + Clone + Debug + Send + 'static;

    fn build(
        &self,
        topic: Topic,
        live_rx: Option<mpsc::Receiver<ToSync<Self::Operation>>>,
        events: mpsc::Sender<Self::Event>,
    ) -> Self::Protocol;
}

/// Interface representing items which arrive from sync sessions and may contain an operation.
///
/// TODO: This is a bit the "other" approach to representing items/events which sometimes carry an
/// operation. We could represent it more like we do in EventDeliveryItem by changing the FromSync
/// type in p2panda-sync.
pub trait SyncSessionItem<OP> {
    fn operation(&self) -> Option<&OP>;
}
