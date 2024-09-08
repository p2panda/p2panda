// SPDX-License-Identifier: AGPL-3.0-or-later

#[cfg(feature = "core")]
mod codec;
#[cfg(feature = "log-height")]
pub mod protocols;
pub mod traits;

// pub use engine::{Engine, Session};

use codec::CborCodec;
use futures::{AsyncRead, AsyncWrite, Sink, Stream};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::{codec::{FramedRead, FramedWrite}, compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt}};

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("input/output error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("codec error: {0}")]
    Codec(String),
    #[error("custom error: {0}")]
    Custom(String),
}


pub fn into_stream<M>(
    rx: Box<dyn AsyncRead + Send + Unpin>,
) -> impl Stream<Item = Result<M, SyncError>> + Send + Unpin
where
    M: for<'a> Deserialize<'a> + Serialize + Send,
{
    FramedRead::new(rx.compat(), CborCodec::<M>::new())
}

pub fn into_sink<M>(
    tx: Box<dyn AsyncWrite + Send + Unpin>,
) -> impl Sink<M, Error = SyncError> + Send + Unpin
where
    M: for<'a> Deserialize<'a> + Serialize + Send,
{
    FramedWrite::new(tx.compat_write(), CborCodec::<M>::new())
}
