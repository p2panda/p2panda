// SPDX-License-Identifier: MIT OR Apache-2.0

use core::task::{Context, Poll};

use futures_util::Stream;
use p2panda_core::{AnyOperation, Topic};

use crate::event_delivery::types::SubscriptionItem;
use crate::streams::StreamFrom;

/// Interface for event delivery solutions.
///
/// This API is intended to represent any source of p2panda operations and related metadata which
/// are delivered to a node for processing. Implementation-wide events and topic stream-specific
/// status updates are also taken into account.
///
/// An event delivery implementation can take many forms. Example solutions include a sync
/// protocol running over QUIC streams, a sneakernet-style sync protocol running over a USB stick
/// or a radio receiving data from nodes in a mesh network.
#[allow(unused)]
pub trait EventDelivery {
    /// Unique identifier for the `EventDelivery` implementation.
    const PROVENANCE: &'static str;

    /// Event delivery stream handle for publishing operations.
    type Publisher: EventDeliveryPublisher;
    /// Event delivery stream handle for receiving items.
    type Subscription: EventDeliverySubscription;

    /// Event delivery ephemeral stream handle for publishing operations.
    type EphemeralPublisher: EventDeliveryPublisher;
    /// Event delivery ephemeral stream handle for receiving items.
    type EphemeralSubscription: EventDeliverySubscription;

    /// Notifications relating to the event delivery implementation.
    type SystemEvent;

    type Error;

    /// Returns a publisher and stateful subscriber for an eventually consistent stream of
    /// messages over the given topic.
    ///
    /// This API is inspired by the principles of "event streaming", combined with eventually
    /// consistent "local-first" and causally ordered events.
    fn stream(
        &self,
        topic: impl Into<Topic>,
    ) -> impl Future<Output = Result<(Self::Publisher, Self::Subscription), Self::Error>>;

    /// Returns a publisher and stateful subscriber for an eventually consistent stream of
    /// messages over the given topic, starting from the given position.
    ///
    /// Use [`StreamFrom`] to determine the starting position of the subscription stream.
    fn stream_from(
        &self,
        topic: impl Into<Topic>,
        from: StreamFrom,
    ) -> impl Future<Output = Result<(Self::Publisher, Self::Subscription), Self::Error>>;

    /// Returns a publisher and subscriber pair for an ephemeral stream of messages over the given
    /// topic.
    ///
    /// Messages sent or received on this stream should not be persisted in local storage. Only
    /// currently online and reachable nodes should receive published messages.
    fn ephemeral_stream(
        &self,
        topic: impl Into<Topic>,
    ) -> impl Future<Output = (Self::EphemeralPublisher, Self::EphemeralSubscription)>;

    /// Returns a stream of system events.
    ///
    /// System events include all system or network-related events, such as space membership
    /// changes or discovery events, which are not associated with a specific topic.
    ///
    /// Any events generated before this method is called should _not_ be emitted.
    fn event_stream(&self) -> impl Stream<Item = Self::SystemEvent>;
}

/// Event delivery stream handle for receiving items.
///
/// Each `EventDeliverySubscription` handle is associated with a particular topic.
///
/// See the [`SubscriptionItem`] documentation for more information.
#[allow(unused)]
pub trait EventDeliverySubscription {
    type Error;

    type Metadata;
    type StatusUpdate;

    /// Returns the topic to which this subscriber is scoped.
    fn topic(&self) -> Topic;

    /// Attempts to receive the next item on the stream.
    #[allow(clippy::type_complexity)]
    fn poll_recv(
        &mut self,
        ctx: &mut Context,
    ) -> Poll<Result<SubscriptionItem<Self::Metadata, Self::StatusUpdate>, Self::Error>>;
}

/// Event delivery stream handle for publishing operations.
///
/// Each `EventDeliveryPublisher` handle is associated with a particular topic.
///
/// While the precise usage is left open to the implementer, this publisher is intended to provide
/// a means for pushing "live" operations which have either been recently forged by the local node
/// or recently received from a remote node.
///
/// An example of the aforemented behaviour can be illustrated by imagining an event delivery
/// solution performing sync over logs. The solution (which implements the [`EventDelivery`] trait)
/// would first synchronise data with a remote node using the latest data from an `OperationStore`.
/// Once this initial replication protocol was concluded, the protocol would enter into a live-mode
/// phase; the `EventDeliveryPublisher` would then be used to send these "hot" operations into the
/// sync session.
#[allow(unused)]
pub trait EventDeliveryPublisher {
    type Error;

    /// Returns the topic to which this publisher is scoped.
    fn topic(&self) -> Topic;

    /// Sends an operation into the event delivery system.
    fn poll_send(
        &self,
        ctx: &mut Context,
        operation: AnyOperation,
    ) -> Poll<Result<(), Self::Error>>;
}
