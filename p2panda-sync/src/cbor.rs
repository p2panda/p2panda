// SPDX-License-Identifier: AGPL-3.0-or-later

use std::marker::PhantomData;

use futures::{AsyncRead, AsyncWrite, Sink, Stream};
use p2panda_core::cbor::{decode_cbor, encode_cbor, DecodeError};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio_util::bytes::{Buf, BytesMut};
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

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let bytes = encode_cbor(&item).map_err(|err| {
            SyncError::Critical(format!("CBOR codec failed encoding message, {err}"))
        })?;
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

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let reader = src.reader();
        let result: Result<Self::Item, _> = decode_cbor(reader);
        match result {
            Ok(item) => Ok(Some(item)),
            Err(ref error) => match error {
                DecodeError::Io(err) => Err(SyncError::Critical(format!(
                    "CBOR codec failed decoding message due to i/o error, {err}"
                ))),
                err => Err(SyncError::InvalidEncoding(err.to_string())),
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
