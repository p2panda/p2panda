// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(unused)] // @TODO: Remove this
mod checkpoint;
mod client;
mod controller;
mod ephemeral;
mod query;
mod stream;

use std::error::Error;
use std::fmt::Debug;

use futures_core::stream::BoxStream;
use p2panda_core::{Hash, Header, Operation};

pub use checkpoint::Checkpoint;
pub use client::{Client, ClientBuilder, ClientError};
pub use query::{Query, QueryError};
use serde::de::IgnoredAny;

/// Special `Operation` type where we don't care what is inside the header extensions.
pub type ErasedOperation = Operation<IgnoredAny>;

pub type ErasedHeader = Header<IgnoredAny>;

pub trait OperationStream: Debug {
    type Error: Error;

    fn subscribe(
        &self,
        query: Query,
        from: Checkpoint,
        live: bool,
    ) -> impl Future<Output = Result<BoxStream<'_, ErasedOperation>, Self::Error>>;

    fn publish(&self, operation: ErasedOperation) -> impl Future<Output = Result<(), Self::Error>>;
}

pub trait EphemeralStream {
    type Error;

    fn subscribe_ephemeral<M>(
        &self,
        topic_id: Hash,
    ) -> impl Future<Output = Result<BoxStream<'_, M>, Self::Error>>;

    fn publish_ephemeral<M>(
        &self,
        topic_id: Hash,
        message: M,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}

pub trait StreamProcessor: Clone + Debug + Send + 'static {
    type Error: Error;

    type Input: Send;

    type Output;

    fn process(
        &self,
        input: Self::Input,
    ) -> impl Future<Output = Result<StreamProcessorOutput<Self::Output>, Self::Error>> + Send;
}

pub enum StreamProcessorOutput<T> {
    Completed(T),
    Deferred,
}

pub trait Transport {
}
