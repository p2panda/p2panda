// SPDX-License-Identifier: MIT OR Apache-2.0

//! Utility methods to encode or decode wire protocol messages in [CBOR] format.
//!
//! [CBOR]: https://cbor.io/
use std::marker::PhantomData;

use futures::{AsyncRead, AsyncWrite, Sink, Stream};
use p2panda_core::cbor::{DecodeError, decode_cbor, encode_cbor};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio_util::bytes::{Buf, BytesMut};
use tokio_util::codec::{Decoder, Encoder};
use tokio_util::codec::{FramedRead, FramedWrite};
use tokio_util::compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt};

use crate::SyncError;

/// Implementation of the tokio codec traits to encode- and decode CBOR data as a stream.
///
/// CBOR allows message framing based on initial "headers" for each "data item", which indicate the
/// type of data and the expected "body" length to be followed. A stream-based decoder can attempt
/// parsing these headers and then reason about if it has enough information to proceed.
///
/// Read more on CBOR in streaming applications here:
/// <https://www.rfc-editor.org/rfc/rfc8949.html#section-5.1>
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

    /// Encodes a serializable item into CBOR bytes and adds them to the buffer.
    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let bytes = encode_cbor(&item).map_err(|err| {
            // When we've failed encoding our _own_ messages something seriously went wrong.
            SyncError::Critical(format!("CBOR codec failed encoding message, {err}"))
        })?;
        // Append the encoded CBOR bytes to the buffer instead of replacing it, we might already
        // have previously encoded items in it.
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

    /// CBOR decoder method taking as an argument the bytes that have been read so far; when called,
    /// it will be in one of the following situations:
    ///
    /// 1. The buffer contains less than a full frame.
    /// 2. The buffer contains exactly a full frame.
    /// 3. The buffer contains more than a full frame.
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Keep a reference of the buffer to not advance the main buffer itself (yet).
        let mut bytes: &[u8] = src.as_ref();
        let starting = bytes.len();

        // Attempt decoding the buffer and remember how many bytes we've advanced it doing that.
        //
        // This will succeed in case 2. and 3.
        let result: Result<Self::Item, _> = decode_cbor(&mut bytes);
        let ending = bytes.len();

        match result {
            Ok(item) => {
                // We've successfully read one full frame from the buffer. We're finally
                // advancing it for the next decode iteration and yield the resulting data item to
                // the stream.
                src.advance(starting - ending);
                Ok(Some(item))
            }
            // Note that the buffer is not further advanced in case of an error.
            Err(ref error) => match error {
                DecodeError::Io(err) => {
                    if err.kind() == std::io::ErrorKind::UnexpectedEof {
                        // EOF errors indicate that our buffer doesn't contain enough data to
                        // decode a whole CBOR frame. We're yielding no data item and re-try
                        // decoding in the next iteration.
                        //
                        // This is handling case 1.
                        Ok(None)
                    } else {
                        // An I/O error during decoding usually indicates something wrong with our
                        // system (lack of system memory etc.).
                        Err(SyncError::Critical(format!(
                            "CBOR codec failed decoding message due to i/o error, {err}"
                        )))
                    }
                }
                err => Err(SyncError::InvalidEncoding(err.to_string())),
            },
        }
    }
}

/// Returns a reader for your data type, automatically decoding CBOR byte-streams and handling the
/// message framing.
///
/// This can be used in various sync protocol implementations where we need to receive data via a
/// wire protocol between two peers.
///
/// This is a convenience method if you want to use CBOR encoding and serde to handle your wire
/// protocol message encoding and framing without implementing it yourself. If you're interested in
/// your own approach you can either implement your own `FramedRead` or `Sink`.
pub fn into_cbor_stream<'a, M>(
    rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
) -> impl Stream<Item = Result<M, SyncError>> + Send + Unpin + 'a
where
    M: for<'de> Deserialize<'de> + Serialize + Send + 'a,
{
    FramedRead::new(rx.compat(), CborCodec::<M>::new())
}

/// Returns a writer for your data type, automatically encoding it as CBOR for a framed
/// byte-stream.
///
/// This can be used in various sync protocol implementations where we need to send data via a wire
/// protocol between two peers.
///
/// This is a convenience method if you want to use CBOR encoding and serde to handle your wire
/// protocol message encoding and framing without implementing it yourself. If you're interested in
/// your own approach you can either implement your own `FramedWrite` or `Stream`.
pub fn into_cbor_sink<'a, M>(
    tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
) -> impl Sink<M, Error = SyncError> + Send + Unpin + 'a
where
    M: for<'de> Deserialize<'de> + Serialize + Send + 'a,
{
    FramedWrite::new(tx.compat_write(), CborCodec::<M>::new())
}

#[cfg(test)]
mod tests {
    use futures::FutureExt;
    use tokio::io::AsyncWriteExt;
    use tokio_stream::StreamExt;
    use tokio_util::codec::FramedRead;

    use super::CborCodec;

    #[tokio::test]
    async fn decoding_exactly_one_frame() {
        let (mut tx, rx) = tokio::io::duplex(64);
        let mut stream = FramedRead::new(rx, CborCodec::<String>::new());

        // CBOR header indicating that a string (6) is followed with the length of 5 bytes.
        // Hexadecimal representation = 65
        // Decimal representation = 101
        tx.write_all(&[101]).await.unwrap();

        // CBOR body, the actual string.
        tx.write_all("hello".as_bytes()).await.unwrap();

        let message = stream.next().await;
        assert_eq!(message, Some(Ok("hello".into())));
    }

    #[tokio::test]
    async fn decoding_more_than_one_frame() {
        let (mut tx, rx) = tokio::io::duplex(64);
        let mut stream = FramedRead::new(rx, CborCodec::<String>::new());

        // CBOR header indicating that a string (6) is followed with the length of 5 bytes.
        // Hexadecimal representation = 65
        // Decimal representation = 101
        tx.write_all(&[101]).await.unwrap();

        // CBOR body, the actual string.
        tx.write_all("hello".as_bytes()).await.unwrap();

        // Another CBOR header (frame) for another message (length of 9).
        // Hexadecimal representation = 69
        // Decimal representation = 105
        tx.write_all(&[105]).await.unwrap();
        tx.write_all("aquariums".as_bytes()).await.unwrap();

        let message = stream.next().await;
        assert_eq!(message, Some(Ok("hello".into())));

        let message = stream.next().await;
        assert_eq!(message, Some(Ok("aquariums".into())));
    }

    #[tokio::test]
    async fn decoding_incomplete_frame() {
        let (mut tx, rx) = tokio::io::duplex(64);
        let mut stream = FramedRead::new(rx, CborCodec::<String>::new());

        // CBOR header indicating that a string (6) is followed with the length of 5 bytes.
        // Hexadecimal representation = 65
        // Decimal representation = 101
        tx.write_all(&[101]).await.unwrap();

        // Attempt to decode an incomplete CBOR frame, the decoder should not yield anything.
        let message = stream.next().now_or_never();
        assert_eq!(message, None);

        // Complete the CBOR data item in the buffer.
        tx.write_all("hello".as_bytes()).await.unwrap();

        let message = stream.next().await;
        assert_eq!(message, Some(Ok("hello".into())));
    }
}
