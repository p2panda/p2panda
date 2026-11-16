// SPDX-License-Identifier: MIT OR Apache-2.0

//! Interfaces for implementing sync protocols.
use std::error::Error as StdError;
use std::fmt::Debug;

use futures_util::Sink;
use futures_util::Stream;
use serde::{Deserialize, Serialize};

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