// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;
use std::pin::Pin;

use futures_util::stream::{Fuse, FusedStream};
use futures_util::task::{Context, Poll};
use futures_util::{Sink, Stream, StreamExt, ready};
use p2panda_core::cbor::{DecodeError, decode_cbor};
use p2panda_core::{Body, Extensions, Header, RawOperation};
use pin_project::pin_project;

use crate::macros::{delegate_access_inner, delegate_sink};

/// An extension trait for `Stream`s that provides a convenient [`decode`](DecodeExt::decode)
/// method.
pub trait DecodeExt<E>: Stream<Item = RawOperation> {
    /// Decode byte streams into p2panda operations.
    fn decode(self) -> Decode<Self, E>
    where
        E: Extensions,
        Self: Sized,
    {
        Decode::new(self)
    }
}

impl<T: ?Sized, E> DecodeExt<E> for T where T: Stream<Item = RawOperation> {}

/// Stream for the [`decode`](DecodeExt::decode) method.
#[derive(Debug)]
#[pin_project]
#[must_use = "streams do nothing unless polled"]
pub struct Decode<St, E>
where
    St: Stream<Item = RawOperation>,
    E: Extensions,
{
    #[pin]
    stream: Fuse<St>,
    _marker: PhantomData<E>,
}

impl<St, E> Decode<St, E>
where
    St: Stream<Item = RawOperation>,
    E: Extensions,
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
    E: Extensions,
{
    type Item = Result<(Header<E>, Option<Body>, Vec<u8>), DecodeError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        let res = ready!(this.stream.as_mut().poll_next(cx));
        Poll::Ready(res.map(|(header_bytes, body_bytes)| {
            match decode_cbor::<Header<E>, _>(&header_bytes[..]) {
                Ok(header) => Ok((header, body_bytes.map(Body::from), header_bytes)),
                Err(err) => Err(err),
            }
        }))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

impl<St: FusedStream, E> FusedStream for Decode<St, E>
where
    St: Stream<Item = RawOperation>,
    E: Extensions,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated()
    }
}

impl<S, E> Sink<RawOperation> for Decode<S, E>
where
    S: Stream<Item = RawOperation> + Sink<RawOperation>,
    E: Extensions,
{
    type Error = S::Error;

    delegate_sink!(stream, RawOperation);
}

#[cfg(test)]
mod tests {
    use futures_util::{StreamExt, TryStreamExt};
    use p2panda_core::{Body, Header};

    use crate::test_utils::{Extensions, mock_stream};

    use super::DecodeExt;

    #[tokio::test]
    async fn decode() {
        let stream = mock_stream().decode();
        let result: Vec<(Header<Extensions>, Option<Body>, Vec<u8>)> =
            stream.take(5).try_collect().await.expect("not fail");
        assert_eq!(result.len(), 5);
    }
}
