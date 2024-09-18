// SPDX-License-Identifier: AGPL-3.0-or-later

use std::marker::PhantomData;
use std::pin::Pin;

use futures_util::stream::{Fuse, FusedStream};
use futures_util::task::{Context, Poll};
use futures_util::{ready, Sink, Stream, StreamExt};
use p2panda_core::{Body, Header};
use pin_project::pin_project;
use serde::de::DeserializeOwned;

use crate::macros::{delegate_access_inner, delegate_sink};
use crate::operation::{decode_operation, DecodeError, RawOperation};

pub trait DecodeExt<E>: Stream<Item = RawOperation> {
    fn decode(self) -> Decode<Self, E>
    where
        E: DeserializeOwned,
        Self: Sized,
    {
        Decode::new(self)
    }
}

impl<T: ?Sized, E> DecodeExt<E> for T where T: Stream<Item = RawOperation> {}

#[derive(Debug)]
#[pin_project]
#[must_use = "streams do nothing unless polled"]
pub struct Decode<St, E>
where
    St: Stream<Item = RawOperation>,
    E: DeserializeOwned,
{
    #[pin]
    stream: Fuse<St>,
    _marker: PhantomData<E>,
}

impl<St, E> Decode<St, E>
where
    St: Stream<Item = RawOperation>,
    E: DeserializeOwned,
{
    pub(super) fn new(stream: St) -> Decode<St, E> {
        Decode {
            stream: stream.fuse(),
            _marker: PhantomData,
        }
    }

    delegate_access_inner!(stream, St, (.));
}

impl<St, E> Stream for Decode<St, E>
where
    St: Stream<Item = RawOperation>,
    E: DeserializeOwned,
{
    type Item = Result<(Header<E>, Option<Body>), DecodeError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        let res = ready!(this.stream.as_mut().poll_next(cx));
        Poll::Ready(res.map(|(header, body)| {
            decode_operation(&header, body.as_ref().map(|body| body.as_ref()))
        }))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

impl<St: FusedStream, E> FusedStream for Decode<St, E>
where
    St: Stream<Item = RawOperation>,
    E: DeserializeOwned,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated()
    }
}

impl<S, E> Sink<RawOperation> for Decode<S, E>
where
    S: Stream<Item = RawOperation> + Sink<RawOperation>,
    E: DeserializeOwned,
{
    type Error = S::Error;

    delegate_sink!(stream, RawOperation);
}

#[cfg(test)]
mod tests {
    use futures_util::{StreamExt, TryStreamExt};
    use p2panda_core::{Body, Header};

    use crate::test_utils::mock_stream;

    use super::DecodeExt;

    #[tokio::test]
    async fn decode() {
        let stream = mock_stream().decode();
        let result: Result<Vec<(Header, Option<Body>)>, _> = stream.take(5).try_collect().await;
        assert!(result.is_ok());
    }
}
