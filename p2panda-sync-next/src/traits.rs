// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{fmt::Debug, pin::Pin};

use futures::Sink;
use futures_util::Stream;
use serde::{Deserialize, Serialize};

use crate::{FromSync, SyncSessionConfig, ToSync};

// @TODO: remove or clarify purpose and use when p2panda-net API is more stable.
//
/// Trait for satisfying requirements coming from the p2panda-net network api.
pub trait NetworkRequirements: Clone + Debug + Send + Sync + 'static {}

impl<T> NetworkRequirements for T where T: Clone + Debug + Send + Sync + 'static {}

/// Generic protocol interface which runs over a typed sink and stream pair.
pub trait Protocol {
    type Output;
    type Error;
    type Event;
    type Message: Serialize + for<'a> Deserialize<'a>;

    fn run(
        self,
        sink: &mut (impl Sink<Self::Message, Error = impl Debug> + Unpin),
        stream: &mut (impl Stream<Item = Result<Self::Message, impl Debug>> + Unpin),
    ) -> impl Future<Output = Result<Self::Output, Self::Error>>;
}

/// Interface for managing sync sessions and consuming events they emit.
#[allow(clippy::type_complexity)]
pub trait SyncManager<T> {
    type Protocol: Protocol;
    type Config: NetworkRequirements;
    type Error: Debug;

    fn from_config(config: Self::Config) -> Self;

    /// Instantiate a new sync session.
    fn session(
        &mut self,
        session_id: u64,
        config: &SyncSessionConfig<T>,
    ) -> impl Future<Output = Self::Protocol>;

    /// Retrieve a send handle to an already existing sync session.
    fn session_handle(
        &self,
        session_id: u64,
    ) -> impl Future<Output = Option<Pin<Box<dyn Sink<ToSync, Error = Self::Error>>>>>;

    /// Subscribe to the manager event stream.
    fn subscribe(
        &self,
    ) -> impl Stream<Item = FromSync<<Self::Protocol as Protocol>::Event>> + Send + Unpin + 'static;
}
