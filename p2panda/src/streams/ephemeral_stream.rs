// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use futures_util::{Stream, StreamExt, ready};
use p2panda_core::cbor::{DecodeError, EncodeError, decode_cbor, encode_cbor};
use p2panda_core::{PrivateKey, PublicKey, Signature};
use p2panda_net::gossip::{GossipHandle, GossipSubscription};
use p2panda_net::timestamp::{HybridTimestamp, LamportTimestamp, Timestamp};
use pin_project::pin_project;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::warn;

use crate::Topic;

/// Message specification version to create and encode messages for ephemeral streams.
const MESSAGE_VERSION: u64 = 1;

/// Message being disseminated to other nodes via gossip.
///
/// They can be seen as a wrapper around the application's message payloads, providing integrity
/// and provenance guarantees, plus making sure each message is unique with the help of a
/// timestamp.
///
/// Messages are represented as a tuple and are CBOR-encoded as follows:
///
/// ```plain
/// (
///    version[u64],
///    public_key[32 bytes],
///    signature[64 bytes],
///    timestamp[u64],
///    lamport_timestamp[u64],
///    body[bytes],
/// )
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
struct WrappedMessage<M> {
    version: u64,
    public_key: PublicKey,
    signature: Signature,
    timestamp: HybridTimestamp,
    body: M,
}

impl<M> WrappedMessage<M>
where
    M: Serialize + for<'a> Deserialize<'a>,
{
    pub fn new(
        body: M,
        timestamp: HybridTimestamp,
        private_key: &PrivateKey,
    ) -> Result<Self, EncodeError> {
        let public_key = private_key.public_key();
        let signature = Self::sign(private_key, public_key, timestamp, &body)?;

        Ok(Self {
            version: MESSAGE_VERSION,
            public_key,
            signature,
            timestamp,
            body,
        })
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, WrappedMessageError> {
        // Attempt deserializing message tuple. This fails if the encoding is invalid.
        let (version, public_key, signature, timestamp, logical, body): (
            u64,
            PublicKey,
            Signature,
            Timestamp,
            LamportTimestamp,
            M,
        ) = decode_cbor(bytes)?;

        let timestamp = HybridTimestamp::from_parts(timestamp, logical);

        // Check supported message version.
        if version != MESSAGE_VERSION {
            return Err(WrappedMessageError::UnsupportedVersion(version));
        }

        let message = Self {
            version,
            public_key,
            signature,
            timestamp,
            body,
        };

        // Check message integrity and provenance.
        message.verify()?;

        Ok(message)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let (timestamp, logical) = self.timestamp.to_parts();
        let message = (
            self.version,
            self.public_key,
            self.signature,
            timestamp,
            logical,
            &self.body,
        );
        let bytes = encode_cbor(&message)?;
        Ok(bytes)
    }

    pub fn verify(&self) -> Result<(), WrappedMessageError> {
        let (timestamp, logical) = self.timestamp.to_parts();
        let message = (
            self.version,
            self.public_key,
            timestamp,
            logical,
            &self.body,
        );

        // Treat encoding error for verifying the signature as an "invalid signature" since
        // the data came from a remote (potentially malicious) node.
        let bytes = encode_cbor(&message).map_err(|_| WrappedMessageError::InvalidSignature)?;

        if !self.public_key.verify(&bytes, &self.signature) {
            return Err(WrappedMessageError::InvalidSignature);
        }

        Ok(())
    }

    fn sign(
        private_key: &PrivateKey,
        public_key: PublicKey,
        timestamp: HybridTimestamp,
        body: &M,
    ) -> Result<Signature, EncodeError> {
        let (timestamp, logical) = timestamp.to_parts();
        let message = (MESSAGE_VERSION, public_key, timestamp, logical, body);
        let bytes = encode_cbor(&message)?;
        Ok(private_key.sign(&bytes))
    }
}

#[derive(Debug, Error)]
enum WrappedMessageError {
    #[error("unsupported message version {0}")]
    UnsupportedVersion(u64),

    #[error("invalid message encoding: {0}")]
    InvalidEncoding(#[from] DecodeError),

    #[error("invalid message signature")]
    InvalidSignature,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EphemeralMessage<M> {
    topic: Topic,
    inner: WrappedMessage<M>,
}

impl<M> EphemeralMessage<M>
where
    M: Serialize + for<'a> Deserialize<'a>,
{
    pub fn topic(&self) -> Topic {
        self.topic
    }

    pub fn author(&self) -> PublicKey {
        self.inner.public_key
    }

    pub fn timestamp(&self) -> u64 {
        // Only return the wall-clock time to the user as this is the interesting bit, the logical
        // lamport timestamp helps internally with keeping messages unique.
        let (timestamp, _logical) = self.inner.timestamp.to_parts();
        timestamp.into()
    }

    pub fn body(&self) -> &M {
        &self.inner.body
    }
}

/// Handle onto an ephemeral stream, exposes API for publishing messages and subscribing to the
/// event stream.
#[derive(Clone)]
pub struct EphemeralStreamHandle<M> {
    topic: Topic,
    private_key: PrivateKey,
    inner: GossipHandle,
    timestamp: Arc<Mutex<HybridTimestamp>>,
    _marker: PhantomData<M>,
}

impl<M> EphemeralStreamHandle<M>
where
    M: Serialize + for<'a> Deserialize<'a>,
{
    pub(crate) fn new(topic: Topic, private_key: PrivateKey, handle: GossipHandle) -> Self {
        Self {
            topic,
            private_key,
            inner: handle,
            timestamp: Arc::new(Mutex::new(HybridTimestamp::now())),
            _marker: PhantomData,
        }
    }

    pub fn topic(&self) -> Topic {
        self.topic
    }

    pub async fn publish(&self, message: M) -> Result<(), PublishError> {
        // The PlumTree implementation for the gossip overlay used by p2panda-net ignores duplicate
        // messages to avoid flooding the network. This can lead to surprises by the users as they
        // expect messages to still arrive, not noticing it's because of a duplicate payload.
        //
        // To help with this we're using a microsecond-precision timestamp + lamport logical clock
        // serving somewhat as a nonce, making sure that every message is guaranteed to be unique.
        let timestamp = {
            let mut timestamp = self
                .timestamp
                .lock()
                .expect("lock poisoned by another thread");
            *timestamp = timestamp.increment();
            *timestamp
        };

        let bytes = {
            let wrapped = WrappedMessage::new(message, timestamp, &self.private_key)?;
            wrapped.to_bytes()?
        };

        self.inner
            .publish(bytes)
            .await
            .map_err(|_err| PublishError::BrokenChannel)?;

        Ok(())
    }

    pub async fn subscribe(&self) -> EphemeralStreamSubscription<M> {
        EphemeralStreamSubscription {
            topic: self.topic,
            inner: self.inner.subscribe(),
            _marker: PhantomData,
        }
    }
}

#[derive(Debug, Error)]
pub enum PublishError {
    /// If this error occurs probably something is wrong with the system.
    #[error("critical encoding error: {0}")]
    Encode(#[from] EncodeError),

    /// Broken / closed communication channel with the internal gossip actor in `p2panda-net`. This
    /// can be due to the actor crashing.
    ///
    /// Users may re-attempt sending the message in case the actor restarted later.
    #[error("error in internal gossip actor occurred")]
    BrokenChannel,
}

#[pin_project]
pub struct EphemeralStreamSubscription<M> {
    topic: Topic,
    #[pin]
    inner: GossipSubscription,
    _marker: PhantomData<M>,
}

impl<M> EphemeralStreamSubscription<M>
where
    M: Serialize + for<'a> Deserialize<'a>,
{
    pub fn topic(&self) -> Topic {
        self.topic
    }
}

impl<M> Stream for EphemeralStreamSubscription<M>
where
    M: Serialize + for<'a> Deserialize<'a>,
{
    type Item = EphemeralMessage<M>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match ready!(self.inner.poll_next_unpin(cx)) {
            // Check encoding & supported version and signature during deserialisation.
            Some(Ok(bytes)) => match WrappedMessage::from_bytes(&bytes) {
                Ok(wrapped) => Poll::Ready(Some(EphemeralMessage {
                    topic: self.topic,
                    inner: wrapped,
                })),
                Err(err) => {
                    // Don't bother users with invalid wrapped messages as this type is not public.
                    // Instead we log a warning, in case this reveals a buggy implementation, etc.
                    warn!("invalid ephemeral message received: {err}");
                    Poll::Pending
                }
            },
            // Ignore internal broadcast channel error, this only indicates that the channel
            // dropped a message which we can't do much about on this layer anymore. In the future
            // we want to remove this error type altogether.
            //
            // Related issue: https://github.com/p2panda/p2panda/issues/959
            Some(Err(_)) => Poll::Pending,
            // Internal stream seized.
            None => Poll::Ready(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::PrivateKey;
    use p2panda_net::timestamp::HybridTimestamp;

    use super::WrappedMessage;

    #[test]
    fn encoding() {
        let private_key = PrivateKey::new();
        let timestamp = HybridTimestamp::now();

        let message_1 = WrappedMessage::new(
            "This message is signed!".to_string(),
            timestamp,
            &private_key,
        )
        .unwrap();

        let bytes = message_1.to_bytes().unwrap();
        let message_2 = WrappedMessage::from_bytes(&bytes).unwrap();

        assert_eq!(message_1, message_2);
    }

    #[test]
    fn signatures() {
        let private_key = PrivateKey::new();
        let timestamp = HybridTimestamp::now();

        let message = WrappedMessage::new(
            "This message is signed!".to_string(),
            timestamp,
            &private_key,
        )
        .unwrap();

        assert!(message.verify().is_ok());
    }
}
