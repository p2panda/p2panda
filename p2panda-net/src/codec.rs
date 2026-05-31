// SPDX-License-Identifier: MIT OR Apache-2.0

//! Utility methods to encode or decode wire protocol messages in [Postcard] format with message
//! framing using 4 bytes frame length prefixes.
//!
//! ## Example
//!
//! Encoding for string "hello" (10 bytes):
//!
//! ```plain
//!           Postcard varint prefix
//!              |
//! Frame Length |
//!  (4 bytes)   |    "hello" string (5 bytes)
//!     |        |          |
//! [----------] | [-h----e----l----l----o-]
//! [0, 0, 0, 6, 5, 104, 101, 108, 108, 111]
//! ============ ===========================
//!    PREFIX              MESSAGE
//! ```
//!
//! [Postcard]: https://postcard.jamesmunns.com/wire-format
use std::marker::PhantomData;

use futures_util::{Sink, Stream};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder, FramedRead, FramedWrite};

/// Implementation of the tokio codec traits to encode- and decode length-prefixed postcard data as
/// a framed stream.
#[derive(Debug)]
pub struct Codec<M> {
    max_frame_len: usize,
    _phantom: PhantomData<M>,
}

impl<M> Codec<M> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn max_frame_len(mut self, value: usize) -> Self {
        self.max_frame_len = value;
        self
    }
}

impl<M> Default for Codec<M> {
    fn default() -> Self {
        Self {
            max_frame_len: 1024 * 1024 * 128, // megabytes
            _phantom: PhantomData,
        }
    }
}

impl<M> Encoder<M> for Codec<M>
where
    M: Serialize,
{
    type Error = CodecError;

    fn encode(&mut self, item: M, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // Find out how large this message will be.
        let frame_len =
            postcard::serialize_with_flavor(&item, postcard::ser_flavors::Size::default())?;
        if frame_len > self.max_frame_len {
            return Err(CodecError::TooLargeMessage(frame_len, self.max_frame_len));
        }

        // Append the encoded frame_len + message bytes to the buffer instead of replacing it, we
        // might already have previously encoded items in it.

        // Encode frame_len prefix (first four bytes) in big-endian order.
        dst.put_u32(u32::try_from(frame_len).expect("already checked"));

        // Increase buffer size for message when necessary.
        dst.reserve(4 + frame_len);

        // Encode message (remaining bytes).
        let mut writer = dst.writer();
        postcard::to_io(&item, &mut writer)?;

        Ok(())
    }
}

impl<M> Decoder for Codec<M>
where
    M: DeserializeOwned,
{
    type Item = M;
    type Error = CodecError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Decode frame length.
        if src.len() < 4 {
            return Ok(None);
        }

        // Keep a reference of the buffer to not advance the main buffer itself (yet).
        let bytes: [u8; 4] = src[..4].try_into().expect("checked available bytes");
        let frame_len = u32::from_be_bytes(bytes) as usize;
        if frame_len > self.max_frame_len {
            return Err(CodecError::TooLargeMessage(frame_len, self.max_frame_len));
        }

        // Decode message.
        if src.len() < 4 + frame_len {
            return Ok(None);
        }
        let item: M = postcard::from_bytes(&src[4..4 + frame_len])?;

        src.advance(4 + frame_len);

        Ok(Some(item))
    }
}

/// Returns a reader for your wire-protocol messages, automatically decoding byte-streams and
/// handling the framing.
pub fn into_codec_stream<M, T>(
    rx: T,
) -> impl Stream<Item = Result<M, CodecError>> + Unpin + use<M, T>
where
    M: for<'de> Deserialize<'de> + 'static,
    T: AsyncRead + Unpin + 'static,
{
    FramedRead::new(rx, Codec::<M>::new())
}

/// Returns a writer for your wire-protocol messages, automatically encoding it as a framed
/// byte-stream.
pub fn into_codec_sink<M, T>(tx: T) -> impl Sink<M, Error = CodecError>
where
    M: Serialize + 'static,
    T: AsyncWrite + Unpin + 'static,
{
    FramedWrite::new(tx, Codec::<M>::new())
}

/// Errors which can occur while decoding or encoding streams of postcard bytes.
#[derive(Debug, Error)]
pub enum CodecError {
    #[error(transparent)]
    Postcard(#[from] postcard::Error),

    #[error("too large message of {0} bytes (max allowed is {1})")]
    TooLargeMessage(usize, usize),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use futures_util::{FutureExt, SinkExt, StreamExt};
    use p2panda_core::test_utils::TestLog;
    use p2panda_core::{Body, Header};
    use tokio::io::AsyncWriteExt;
    use tokio_util::codec::{FramedRead, FramedWrite};

    use super::{Codec, into_codec_sink, into_codec_stream};

    #[tokio::test]
    async fn decoding_exactly_one_frame() {
        let (mut tx, rx) = tokio::io::duplex(64);
        let mut stream = FramedRead::new(rx, Codec::<String>::new());

        // Frame-length (big-endian) of 6 bytes (1 byte postcard varint + 5 byte string).
        tx.write_all(&[0, 0, 0, 6]).await.unwrap();

        // Postcard prefix for indicating the string length.
        tx.write_all(&[5]).await.unwrap();

        // Message, the actual string.
        tx.write_all("hello".as_bytes()).await.unwrap();

        let message = stream.next().await;
        assert_eq!(message.unwrap().unwrap(), "hello".to_string());
    }

    #[tokio::test]
    async fn decoding_more_than_one_frame() {
        let (mut tx, rx) = tokio::io::duplex(64);
        let mut stream = FramedRead::new(rx, Codec::<String>::new());

        // Frame-length (big-endian) of 6 bytes (1 byte postcard varint + 5 byte string).
        tx.write_all(&[0, 0, 0, 6]).await.unwrap();

        tx.write_all(&[5]).await.unwrap();
        tx.write_all("hello".as_bytes()).await.unwrap();

        // Another frame for another message of 10 bytes (1 byte postcard varint + 9 byte string).
        tx.write_all(&[0, 0, 0, 10]).await.unwrap();

        tx.write_all(&[9]).await.unwrap();
        tx.write_all("aquariums".as_bytes()).await.unwrap();

        let message = stream.next().await;
        assert_eq!(message.unwrap().unwrap(), "hello".to_string());

        let message = stream.next().await;
        assert_eq!(message.unwrap().unwrap(), "aquariums".to_string());
    }

    #[tokio::test]
    async fn decoding_incomplete_frame() {
        let (mut tx, rx) = tokio::io::duplex(64);
        let mut stream = FramedRead::new(rx, Codec::<String>::new());

        // Frame-length (big-endian) of 6 bytes (1 byte postcard varint + 5 byte string).
        tx.write_all(&[0, 0, 0, 6]).await.unwrap();

        // Attempt to decode an incomplete frame, the decoder should not yield anything.
        let message = stream.next().now_or_never();
        assert!(message.is_none());

        // Complete the data item in the buffer.
        tx.write_all(&[5]).await.unwrap();
        tx.write_all("h".as_bytes()).await.unwrap();
        tx.write_all("ello".as_bytes()).await.unwrap();

        let message = stream.next().await;
        assert_eq!(message.unwrap().unwrap(), "hello".to_string());
    }

    #[tokio::test]
    async fn decoding_too_large_message() {
        let (mut tx, rx) = tokio::io::duplex(64);
        let mut stream = FramedRead::new(rx, Codec::<String>::new().max_frame_len(4));

        tx.write_all(&[0, 0, 0, 6]).await.unwrap();
        tx.write_all(&[5]).await.unwrap();
        tx.write_all("hello".as_bytes()).await.unwrap();

        let message = stream.next().await;
        assert!(message.unwrap().is_err());
    }

    #[tokio::test]
    async fn encoding_too_large_message() {
        let (tx, _rx) = tokio::io::duplex(64);
        let mut sink = FramedWrite::new(tx, Codec::<String>::new().max_frame_len(4));
        assert!(sink.send("hello".into()).await.is_err());
    }

    #[tokio::test]
    async fn encoding() {
        let (tx, _rx) = tokio::io::duplex(64);
        let mut sink = FramedWrite::new(tx, Codec::<String>::new());
        assert!(sink.feed("hello".into()).await.is_ok());
        assert!(sink.feed("hello".into()).await.is_ok());
        assert!(sink.feed("hello".into()).await.is_ok());
        assert!(sink.flush().await.is_ok());
    }

    #[tokio::test]
    async fn operations_stream() {
        type Payload = (Header<u32>, Option<Body>);

        // Give stream a large enough buffer size since we're creating all messages up-front before
        // consuming them.
        let (tx_inner, rx_inner) = tokio::io::duplex(1024 * 100);

        let mut tx = into_codec_sink::<Payload, _>(tx_inner);
        let mut rx = into_codec_stream::<Payload, _>(rx_inner);

        // Create 100 operations, encode and send bytes to receiver.
        let log = TestLog::new();
        for _ in 0..100 {
            let operation = log.operation(b"boom boom boom", 32);
            tx.send((operation.header, operation.body)).await.unwrap();
        }

        // Receiver writes bytes into buffer, attempts decoding and returns header/body tuple 100
        // times.
        let mut i = 1;
        loop {
            if let Some(message) = rx.next().await {
                if let Err(err) = message {
                    panic!("{err}");
                }

                i += 1;

                if i == 100 {
                    break;
                }
            }
        }
    }
}
