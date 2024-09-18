// SPDX-License-Identifier: AGPL-3.0-or-later

use std::pin::Pin;

use futures_channel::mpsc;
use futures_util::stream::{Fuse, FusedStream};
use futures_util::task::{Context, Poll};
use futures_util::{ready, Sink, Stream, StreamExt};
use p2panda_core::{Body, Header, Operation};
use pin_project_lite::pin_project;
use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::macros::{delegate_access_inner, delegate_sink};

pub trait IngestExt<E>: Stream<Item = (Header<E>, Option<Body>)> {
    fn ingest(self) -> Ingest<Self, E>
    where
        E: DeserializeOwned,
        Self: Sized,
    {
        Ingest::new(self)
    }
}

impl<T: ?Sized, E> IngestExt<E> for T where T: Stream<Item = (Header<E>, Option<Body>)> {}

pin_project! {
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct Ingest<St, E>
    where
        St: Stream<Item = (Header<E>, Option<Body>)>,
        E: DeserializeOwned,
    {
        #[pin]
        stream: Fuse<St>,
    }
}

impl<St, E> Ingest<St, E>
where
    St: Stream<Item = (Header<E>, Option<Body>)>,
    E: DeserializeOwned,
{
    pub(super) fn new(stream: St) -> Ingest<St, E> {
        // let (_ooo_buffer_tx, mut ooo_buffer_rx) = mpsc::channel::<Operation<_>>(128);

        Ingest {
            stream: stream.fuse(),
        }
    }

    delegate_access_inner!(stream, St, (.));
}

impl<St, E> Stream for Ingest<St, E>
where
    St: Stream<Item = (Header<E>, Option<Body>)>,
    E: DeserializeOwned,
{
    type Item = Result<Operation, IngestError>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        Poll::Pending
    }
}

impl<St: FusedStream, E> FusedStream for Ingest<St, E>
where
    St: Stream<Item = (Header<E>, Option<Body>)>,
    E: DeserializeOwned,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated()
    }
}

impl<S, E> Sink<(Header<E>, Option<Body>)> for Ingest<S, E>
where
    S: Stream<Item = (Header<E>, Option<Body>)> + Sink<(Header<E>, Option<Body>)>,
    E: DeserializeOwned,
{
    type Error = S::Error;

    delegate_sink!(stream, (Header<E>, Option<Body>));
}

#[derive(Debug, Error)]
pub enum IngestError {}

#[cfg(test)]
mod tests {}
