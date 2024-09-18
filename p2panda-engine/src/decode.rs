// SPDX-License-Identifier: AGPL-3.0-or-later

use std::pin::Pin;

use futures_util::stream::{Fuse, FusedStream};
use futures_util::task::{Context, Poll};
use futures_util::{ready, Sink, Stream, StreamExt};
use p2panda_core::{Body, Header};
use pin_project_lite::pin_project;

use crate::macros::{delegate_access_inner, delegate_sink};
use crate::operation::{decode_operation, DecodeError, RawOperation};

pub trait DecodeExt: Stream<Item = RawOperation> {
    fn decode(self) -> Decode<Self>
    where
        Self: Sized,
    {
        Decode::new(self)
    }
}

impl<T: ?Sized> DecodeExt for T where T: Stream<Item = RawOperation> {}

pin_project! {
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct Decode<St>
    where
        St: Stream<Item = RawOperation>,
    {
        #[pin]
        stream: Fuse<St>,
    }
}

impl<St> Decode<St>
where
    St: Stream<Item = RawOperation>,
{
    pub(super) fn new(stream: St) -> Decode<St> {
        Decode {
            stream: stream.fuse(),
        }
    }

    delegate_access_inner!(stream, St, (.));
}

impl<St> Stream for Decode<St>
where
    St: Stream<Item = RawOperation>,
{
    type Item = Result<(Header, Option<Body>), DecodeError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        let res = ready!(this.stream.as_mut().poll_next(cx));
        Poll::Ready(res.map(|(header, body)| {
            decode_operation(&header, body.as_ref().map(|body| body.as_ref()))
        }))
    }
}

impl<St: FusedStream> FusedStream for Decode<St>
where
    St: Stream<Item = RawOperation>,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated()
    }
}

impl<S> Sink<RawOperation> for Decode<S>
where
    S: Stream<Item = RawOperation> + Sink<RawOperation>,
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
