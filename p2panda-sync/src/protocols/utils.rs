// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{protocols::cbor_codec::CborCodec, SyncError};
use futures::{AsyncRead, AsyncWrite, Sink, Stream};
use serde::{Deserialize, Serialize};
use tokio_util::codec::{FramedRead, FramedWrite};
use tokio_util::compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt};

pub fn into_stream<'a, M>(
    rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
) -> impl Stream<Item = Result<M, SyncError>> + Send + Unpin + 'a
where
    M: for<'de> Deserialize<'de> + Serialize + Send + 'a,
{
    FramedRead::new(rx.compat(), CborCodec::<M>::new())
}

pub fn into_sink<'a, M>(
    tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
) -> impl Sink<M, Error = SyncError> + Send + Unpin + 'a
where
    M: for<'de> Deserialize<'de> + Serialize + Send + 'a,
{
    FramedWrite::new(tx.compat_write(), CborCodec::<M>::new())
}
