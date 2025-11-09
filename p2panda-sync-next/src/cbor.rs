// SPDX-License-Identifier: MIT OR Apache-2.0

//! Utility methods to encode or decode wire protocol messages in [CBOR] format.
//!
//! [CBOR]: https://cbor.io/
use std::marker::PhantomData;

use futures::{AsyncRead, AsyncWrite, Sink, Stream};
use p2panda_core::cbor::{DecodeError, EncodeError, decode_cbor, encode_cbor};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::bytes::{Buf, BytesMut};
use tokio_util::codec::{Decoder, Encoder};
use tokio_util::codec::{FramedRead, FramedWrite};
use tokio_util::compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt};

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
    type Error = CborCodecError;

    /// Encodes a serializable item into CBOR bytes and adds them to the buffer.
    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // NOTE: If we've failed encoding our _own_ messages something seriously went wrong.
        let bytes = encode_cbor(&item)?;
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
    type Error = CborCodecError;

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
            Err(error) => match error {
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
                        Err(CborCodecError::IO(format!(
                            "CBOR codec failed decoding message due to i/o error, {err}"
                        )))
                    }
                }
                err => Err(CborCodecError::Decode(err)),
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
pub fn into_cbor_stream<M, T>(
    rx: T,
) -> impl Stream<Item = Result<M, CborCodecError>> + Unpin + use<M, T>
where
    M: for<'de> Deserialize<'de> + Serialize + 'static,
    T: AsyncRead + Unpin + 'static,
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
pub fn into_cbor_sink<M, T>(tx: T) -> impl Sink<M, Error = CborCodecError>
where
    M: for<'de> Deserialize<'de> + Serialize + 'static,
    T: AsyncWrite + Unpin + 'static,
{
    FramedWrite::new(tx.compat_write(), CborCodec::<M>::new())
}

/// Errors which can occur while decoding or encoding streams of cbor bytes.
#[derive(Debug, Error)]
pub enum CborCodecError {
    #[error(transparent)]
    Decode(#[from] DecodeError),

    #[error(transparent)]
    Encode(#[from] EncodeError),

    #[error("{0}")]
    IO(String),

    #[error("{0}")]
    BrokenPipe(String),
}

/// Converts critical I/O error (which occurs during codec stream handling) into [`SyncError`].
///
/// This is usually a critical system failure indicating an implementation bug or lacking resources
/// on the user's machine.
///
/// See `Encoder` or `Decoder` `Error` trait type in tokio's codec for more information:
/// <https://docs.rs/tokio-util/latest/tokio_util/codec/trait.Decoder.html#associatedtype.Error>
impl From<std::io::Error> for CborCodecError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            // Broken pipes usually indicate that the remote peer closed the connection
            // unexpectedly, this is why we're not treating it as a critical error but as
            // "unexpected behaviour" instead.
            std::io::ErrorKind::BrokenPipe => Self::BrokenPipe("broken pipe".into()),
            _ => Self::IO(format!("internal i/o stream error {err}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use assert_matches::assert_matches;
    use futures::FutureExt;
    use p2panda_core::{Body, Operation};
    use tokio::io::AsyncWriteExt;
    use tokio_stream::StreamExt;
    use tokio_util::codec::FramedRead;
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    use crate::cbor::{into_cbor_sink, into_cbor_stream};
    use crate::log_sync::{LogSyncEvent, StatusEvent};
    use crate::test_utils::{Peer, TestTopic, TestTopicSyncEvent};
    use crate::topic_handshake::TopicHandshakeEvent;
    use crate::topic_log_sync::Role;
    use crate::traits::Protocol;

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
        assert_matches!(message, Some(Ok(message)) if message == "hello".to_string());
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
        assert_matches!(message, Some(Ok(message)) if message == "hello".to_string());

        let message = stream.next().await;
        assert_matches!(message, Some(Ok(message)) if message == "aquariums".to_string());
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
        assert_matches!(message, None);

        // Complete the CBOR data item in the buffer.
        tx.write_all("hello".as_bytes()).await.unwrap();

        let message = stream.next().await;
        assert_matches!(message, Some(Ok(message)) if message == "hello".to_string());
    }

    #[tokio::test]
    async fn topic_log_sync_over_byte_streams() {
        let topic = TestTopic::new("messages");
        let log_id = 0;

        let mut peer_a = Peer::new(0);
        let mut peer_b = Peer::new(1);

        let body = Body::new("Hello, Sloth!".as_bytes());
        let (header_0, _) = peer_a.create_operation(&body, 0).await;
        let (header_1, _) = peer_a.create_operation(&body, 0).await;
        let (header_2, _) = peer_a.create_operation(&body, 0).await;

        let logs = HashMap::from([(peer_a.id(), vec![log_id])]);
        peer_a.insert_topic(&topic, &logs);

        let (peer_a_session, mut peer_a_events_rx, _) =
            peer_a.topic_sync_protocol(Role::Initiate(topic.clone()), false);

        let (peer_b_session, mut peer_b_events_rx, _) =
            peer_b.topic_sync_protocol(Role::Accept, false);

        let (peer_a_bi_streams, peer_b_bi_streams) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a_bi_streams);
        let peer_a_read = peer_a_read.compat();
        let peer_a_write = peer_a_write.compat_write();
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b_bi_streams);
        let peer_b_read = peer_b_read.compat();
        let peer_b_write = peer_b_write.compat_write();

        // Convert generic read-write channels into framed sink and stream of cbor encoded protocol messages.
        let mut peer_a_sink = into_cbor_sink(peer_a_write);
        let mut peer_a_stream = into_cbor_stream(peer_a_read);
        let mut peer_b_sink = into_cbor_sink(peer_b_write);
        let mut peer_b_stream = into_cbor_stream(peer_b_read);

        let peer_a_task = tokio::spawn(async move {
            peer_a_session
                .run(&mut peer_a_sink, &mut peer_a_stream)
                .await
        });

        let peer_b_task = tokio::spawn(async move {
            peer_b_session
                .run(&mut peer_b_sink, &mut peer_b_stream)
                .await
        });
        let (peer_a_result, peer_b_result) = tokio::try_join!(peer_a_task, peer_b_task).unwrap();
        assert!(peer_a_result.is_ok());
        assert!(peer_b_result.is_ok());

        let mut index = 0;
        while let Some(event) = peer_a_events_rx.next().await {
            match index {
                0 => assert_matches!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Initiate(
                        sent_topic,
                    ))
                    if sent_topic == topic
                ),
                1 => assert_eq!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Done(topic.clone()))
                ),
                2 => assert_matches!(
                    event,
                    TestTopicSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Started { .. }),)
                ),
                3 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ),)
                    );
                }
                4 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ),)
                    );
                }
                5 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Completed { .. }
                        ),)
                    );
                    break;
                }
                _ => panic!(),
            };
            index += 1;
        }

        let mut index = 0;
        while let Some(event) = peer_b_events_rx.next().await {
            match index {
                0 => assert_eq!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Accept)
                ),
                1 => assert_matches!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::TopicReceived(received_topic))
                    if received_topic == topic
                ),
                2 => assert_eq!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Done(topic.clone()))
                ),
                3 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Started { .. }))
                    );
                }
                4 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ))
                    );
                }
                5 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ))
                    );
                }
                6 => {
                    let (header, body_inner) = assert_matches!(
                    event,
                    TestTopicSyncEvent::Sync (
                        LogSyncEvent::Data(operation)
                    ) => {let Operation {header, body, ..} = *operation; (header, body)});
                    assert_eq!(header, header_0);
                    assert_eq!(body_inner.unwrap(), body);
                }
                7 => {
                    let (header, body_inner) = assert_matches!(
                    event,
                    TestTopicSyncEvent::Sync (
                        LogSyncEvent::Data(operation)
                    ) => {let Operation {header, body, ..} = *operation; (header, body)});
                    assert_eq!(header, header_1);
                    assert_eq!(body_inner.unwrap(), body);
                }
                8 => {
                    let (header, body_inner) = assert_matches!(
                    event,
                    TestTopicSyncEvent::Sync (
                        LogSyncEvent::Data(operation)
                    ) => {let Operation {header, body, ..} = *operation; (header, body)});
                    assert_eq!(header, header_2);
                    assert_eq!(body_inner.unwrap(), body);
                }
                9 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Completed { .. }
                        ),)
                    );
                    break;
                }
                _ => panic!(),
            };
            index += 1;
        }
    }
}
