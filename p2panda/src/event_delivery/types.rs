// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::AnyOperation;

/// Operation received from an event delivery implementation.
///
/// The operation may be an encrypted cyphertext (in the form of a byte array) or a plaintext
/// [`AnyOperation`].
///
/// [`AnyOperation`]: <https://docs.rs/p2panda-core/latest/p2panda_core/operation/struct.AnyOperation.html>
#[allow(unused)]
pub enum EventDeliveryOperation {
    Encrypted(Vec<u8>),
    Plaintext(Box<AnyOperation>),
}

/// Item emitted by an event delivery implementation.
///
/// The `SubscriptionItem` defines two variant; one consists of an [`EventDeliveryOperation`] and
/// the other supplies some generic `StatusUpdate` type.
///
/// These items are emitted by types implementing the [`EventDeliverySubscription`] trait. As such,
/// they are intended to be scoped to a particular topic stream. This means that all subscription
/// items emitted by an [`EventDeliverySubscription`] stream will be for the same topic.
///
/// The `Operation` variant represents a p2panda operation, either as an encrypted byte array or an
/// [`AnyOperation`]. This variant also includes some generic metadata. The metadata might include
/// the ID of the remote node from which this operation was received, for example.
///
/// The `StatusUpdate` variant is intended to represent events which occur during the life-cycle of
/// the underlying event delivery solution, and which have direct relevance to the topic of the
/// stream on which they're emitted. For example, a `SubscriptionItem::StatusUpdate` emitted by a
/// sync protocol may indicate that a sync session has started or ended for the associated topic.
/// Updates which provide information about the entire event delivery implementation should instead
/// be emitted on the [`EventDelivery::event_stream()`].
///
/// [`EventDeliverySubscription`]: crate::event_delivery::traits::EventDeliverySubscription
/// [`AnyOperation`]: <https://docs.rs/p2panda-core/latest/p2panda_core/operation/struct.AnyOperation.html>
/// [`EventDelivery::event_stream()`]: crate::event_delivery::traits::EventDelivery::event_stream()
#[allow(unused)]
pub enum SubscriptionItem<MD, S> {
    Operation {
        operation: Box<EventDeliveryOperation>,
        meta_data: MD,
    },
    StatusUpdate(S),
}
