// SPDX-License-Identifier: AGPL-3.0-or-later

use std::marker::PhantomData;

use futures::{AsyncRead, AsyncWrite, Sink, Stream};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio_util::bytes::Buf;
use tokio_util::codec::{Decoder, Encoder};
use tokio_util::codec::{FramedRead, FramedWrite};
use tokio_util::compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt};

use crate::SyncError;

#[derive(Clone, Debug)]
pub struct CborCodec<T> {
    _phantom: PhantomData<T>,
}

impl<M> CborCodec<M> {
    pub fn new() -> Self {
        CborCodec {
            _phantom: PhantomData {},
        }
    }
}

impl<M> Default for CborCodec<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Encoder<T> for CborCodec<T>
where
    T: Serialize,
{
    type Error = SyncError;

    fn encode(
        &mut self,
        item: T,
        dst: &mut tokio_util::bytes::BytesMut,
    ) -> Result<(), Self::Error> {
        let mut bytes = Vec::new();
        ciborium::into_writer(&item, &mut bytes).map_err(|e| SyncError::Codec(e.to_string()))?;
        dst.extend_from_slice(&bytes);
        Ok(())
    }
}

impl<T> Decoder for CborCodec<T>
where
    T: Serialize + DeserializeOwned,
{
    type Item = T;
    type Error = SyncError;

    fn decode(
        &mut self,
        src: &mut tokio_util::bytes::BytesMut,
    ) -> Result<Option<Self::Item>, Self::Error> {
        let reader = src.reader();
        let result: Result<Self::Item, _> = ciborium::from_reader(reader);
        match result {
            // If we read the item, we also need to advance the underlying buffer.
            Ok(item) => Ok(Some(item)),
            Err(ref error) => match error {
                // Sometimes the EOF is signalled as IO error
                ciborium::de::Error::Io(_) => Ok(None),
                e => Err(SyncError::Codec(e.to_string())),
            },
        }
    }
}

pub fn into_cbor_stream<'a, M>(
    rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
) -> impl Stream<Item = Result<M, SyncError>> + Send + Unpin + 'a
where
    M: for<'de> Deserialize<'de> + Serialize + Send + 'a,
{
    FramedRead::new(rx.compat(), CborCodec::<M>::new())
}

pub fn into_cbor_sink<'a, M>(
    tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
) -> impl Sink<M, Error = SyncError> + Send + Unpin + 'a
where
    M: for<'de> Deserialize<'de> + Serialize + Send + 'a,
{
    FramedWrite::new(tx.compat_write(), CborCodec::<M>::new())
}
