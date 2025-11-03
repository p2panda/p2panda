// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;
use std::fmt::Debug;
use std::future::Future;

use futures_core::Stream;
use p2panda_core::Hash;
use serde::{Deserialize, Serialize};

use crate::{Checkpoint, Subject};

// @TODO: Can we use this as a general API for node-node ("sync") and node-client communication?
// Then we might want to move this definition into p2panda-sync?
pub trait Connector: Send + Sync + 'static {
    type Error: Error;

    // NOTE(adz): The Debug trait bound is strictly speaking not necessary. Because of the use of
    // more complex error types with generics, the Rust compiler can't detect that Self::Error
    // should already implement Debug using the Error supertrait and fails to compile the parent
    // struct, Subscription instead. This is annoying, but adding Debug here doesn't hurt either.
    type Subscription: Subscription + Debug;

    fn subscribe(
        &self,
        subject: Subject,
        from: Checkpoint,
        live: bool,
    ) -> impl Future<Output = Result<Self::Subscription, Self::Error>> + Send;

    fn publish(
        &self,
        // @TODO: Not sure yet if we will have the subject as the log id in the header or not.
        subject: Subject,
        header: Vec<u8>,
        body: Vec<u8>,
    ) -> impl Future<Output = Result<Hash, Self::Error>> + Send;
}

pub trait Subscription: Send + Sync {
    type Error: Error;

    type EventStream: Stream<Item = Result<StreamEvent, Self::Error>> + Send + Unpin;

    fn id(&self) -> SubscriptionId;

    fn events(&self) -> Self::EventStream;

    fn replay(&mut self, from: Checkpoint) -> impl Future<Output = Result<(), Self::Error>> + Send;

    fn unsubscribe(self) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

pub type SubscriptionId = u64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamEvent {
    Subscribed {
        subscription_id: SubscriptionId,
    },
    Operation {
        id: Hash,
        header: Vec<u8>,
        body: Option<Vec<u8>>,
    },
    Unsubscribed,
}
