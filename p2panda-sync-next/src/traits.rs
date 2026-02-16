// SPDX-License-Identifier: MIT OR Apache-2.0

//! Interfaces for implementing sync protocols and managers.
use std::error::Error as StdError;
use std::fmt::Debug;
use std::pin::Pin;

use futures::Sink;
use futures_util::Stream;
use serde::{Deserialize, Serialize};

use crate::{FromSync, SessionConfig, ToSync};

/// Generic protocol interface which runs over a sink and stream pair.
pub trait Protocol {
    type Output;
    type Error: StdError + Send + Sync + 'static;
    type Message: Serialize + for<'a> Deserialize<'a>;

    fn run(
        self,
        sink: &mut (impl Sink<Self::Message, Error = impl Debug> + Unpin),
        stream: &mut (impl Stream<Item = Result<Self::Message, impl Debug>> + Unpin),
    ) -> impl Future<Output = Result<Self::Output, Self::Error>>;
}

/// Interface for managing sync sessions and consuming events they emit.
#[allow(clippy::type_complexity)]
pub trait Manager<T> {
    type Protocol: Protocol + Send + 'static;
    type Args: Clone + Send + 'static;
    type Message: Clone + Send + 'static;
    type Event: Clone + Debug + Send + 'static;
    type Error: StdError + Send + Sync + 'static;

    fn from_args(args: Self::Args) -> Self;

    /// Instantiate a new sync session.
    fn session(
        &mut self,
        session_id: u64,
        config: &SessionConfig<T>,
    ) -> impl Future<Output = Self::Protocol>;

    /// Retrieve a send handle to an already existing sync session.
    fn session_handle(
        &self,
        session_id: u64,
    ) -> impl Future<Output = Option<Pin<Box<dyn Sink<ToSync<Self::Message>, Error = Self::Error>>>>>;

    /// Subscribe to the manager event stream.
    fn subscribe(&mut self) -> impl Stream<Item = FromSync<Self::Event>> + Send + Unpin + 'static;
}

/// Maps a topic to a user defined data type being sent over the wire during sync.
///
/// It defines the type of data it is expecting to sync and how the scope for a particular session
/// should be identified; users provide an implementation of the `TopicMap` trait in order to
/// define how this mapping occurs.
///
/// Since `TopicMap` is generic we can use the same mapping across different sync implementations
/// for the same data type when necessary.
///
/// For example a `TopicMap` map implementation could map a generic `T` to a set of logs.
///
/// ## Designing `TopicMap` for applications
///
/// Considering an example chat application which is based on append-only log data types, we
/// probably want to organise messages from an author for a certain chat group into one log each.
/// Like this, a chat group can be expressed as a collection of one to potentially many logs (one
/// per member of the group):
///
/// ```text
/// All authors: A, B and C
/// All chat groups: 1 and 2
///
/// "Chat group 1 with members A and B"
/// - Log A1
/// - Log B1
///
/// "Chat group 2 with members A, B and C"
/// - Log A2
/// - Log B2
/// - Log C2
/// ```
///
/// If we implement `T` to express that we're interested in syncing over a specific chat group,
/// for example "Chat Group 2" we would implement `TopicMap` to give us all append-only logs of
/// all members inside this group, that is the entries inside logs `A2`, `B2` and `C2`.
pub trait TopicMap<T, V>: Clone {
    type Error: StdError;

    fn get(&self, topic: &T) -> impl Future<Output = Result<V, Self::Error>>;
}
