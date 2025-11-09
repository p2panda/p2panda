// SPDX-License-Identifier: MIT OR Apache-2.0

use std::hash::Hash as StdHash;
use std::{fmt::Debug, pin::Pin};

use futures::Sink;
use futures_util::Stream;
use serde::{Deserialize, Serialize};

use crate::{SyncManagerEvent, SyncSessionConfig, ToSync};

/// Generic protocol interface which runs over a typed sink and stream pair.
pub trait Protocol {
    type Output;
    type Error;
    type Event;
    type Message;

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
    type Error: Debug;

    /// Instantiate a new sync session.
    fn session(&mut self, session_id: u64, config: &SyncSessionConfig<T>) -> Self::Protocol;

    /// Retrieve a send handle to an already existing sync session.
    fn session_handle(
        &self,
        session_id: u64,
    ) -> Option<Pin<Box<dyn Sink<ToSync, Error = Self::Error>>>>;

    /// Drive the manager to process and return events emitted from all running sync sessions.
    fn next_event(
        &mut self,
    ) -> impl Future<
        Output = Result<
            Option<SyncManagerEvent<T, <Self::Protocol as Protocol>::Event>>,
            Self::Error,
        >,
    >;
}

/// Identify the particular dataset a peer is interested in syncing.
///
/// Exactly how this is expressed is left up to the user to decide. During sync the "initiator"
/// sends their topic query to a remote peer where it is be mapped to their local dataset.
/// Additional access-control checks can be performed. Once this "handshake" is complete both
/// peers will proceed with the designated sync protocol.
///
/// ## `TopicId` vs `TopicQuery`
///
/// While `TopicId` is merely a 32-byte identifier which can't hold much information other than
/// being a distinct identifier of a single data item or collection of them, we can use `TopicQuery` to
/// implement custom data types representing "queries" for very specific data items. Peers can for
/// example announce that they'd like "all events from the 27th of September 23 until today" with
/// `TopicQuery`.
///
/// Consult the `TopicId` documentation in `p2panda-net` for more information.
pub trait TopicQuery:
    // Data types implementing `TopicQuery` also need to implement `Eq` and `Hash` in order to allow 
    // backends to organise sync sessions per topic query and peer, along with `Serialize` and 
    // `Deserialize` to allow sending topics over the wire.
    Clone + Debug + Eq + StdHash + Serialize + for<'a> Deserialize<'a>
{
}
