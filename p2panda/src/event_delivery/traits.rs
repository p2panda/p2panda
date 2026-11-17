// SPDX-License-Identifier: MIT OR Apache-2.0

use core::task::{Context, Poll};

use futures_util::Stream;
use p2panda_core::{AnyOperation, Topic};

use crate::event_delivery::types::SubscriptionItem;
use crate::streams::StreamFrom;

#[allow(unused)]
pub trait EventDelivery {
    const PROVENANCE: &'static str;

    type Publisher: EventDeliveryPublisher;
    type Subscription: EventDeliverySubscription;

    type EphemeralPublisher: EventDeliveryPublisher;
    type EphemeralSubscription: EventDeliverySubscription;

    type SystemEvent;

    type Error;

    fn stream(
        &self,
        topic: impl Into<Topic>,
    ) -> impl Future<Output = Result<(Self::Publisher, Self::Subscription), Self::Error>>;

    fn stream_from(
        &self,
        topic: impl Into<Topic>,
        from: StreamFrom,
    ) -> impl Future<Output = Result<(Self::Publisher, Self::Subscription), Self::Error>>;

    fn ephemeral_stream(
        &self,
        topic: impl Into<Topic>,
    ) -> impl Future<Output = (Self::EphemeralPublisher, Self::EphemeralSubscription)>;

    fn event_stream(&self) -> impl Stream<Item = Self::SystemEvent>;
}

#[allow(unused)]
pub trait EventDeliverySubscription {
    type Error;

    type Metadata;
    type StatusUpdate;

    fn topic(&self) -> Topic;

    #[allow(clippy::type_complexity)]
    fn poll_recv(
        &mut self,
        ctx: &mut Context,
    ) -> Poll<Result<SubscriptionItem<Self::Metadata, Self::StatusUpdate>, Self::Error>>;
}

#[allow(unused)]
pub trait EventDeliveryPublisher {
    type Error;

    fn topic(&self) -> Topic;

    fn poll_send(
        &self,
        ctx: &mut Context,
        operation: AnyOperation,
    ) -> Poll<Result<(), Self::Error>>;
}
