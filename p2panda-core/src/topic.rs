// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Display;
use std::hash::Hash as StdHash;
use std::str::FromStr;

use rand::Rng;
use rand::rngs::OsRng;
use thiserror::Error;

use crate::{Hash, PublicKey};

pub const TOPIC_LENGTH: usize = 32;

/// Identifier for a gossip- or sync topic.
///
/// A topic identifier is required when subscribing or publishing to a stream.
///
/// Topics usually describe concrete data which nodes want to exchange over, for example a document
/// id or chat group id and so forth. Applications usually want to share topics via a secure side
/// channel.
///
/// **WARNING:** Sensitive topics have to be treated like secret values and generated using a
/// cryptographically secure pseudorandom number generator (CSPRNG). Otherwise they can be easily
/// guessed by third parties or leaked during discovery.
#[derive(Clone, Copy, Debug, Ord, PartialOrd, PartialEq, Eq, StdHash)]
pub struct Topic(pub(crate) [u8; TOPIC_LENGTH]);

impl Topic {
    pub fn new() -> Self {
        let mut rng = OsRng;
        Self::from_rng(&mut rng)
    }

    pub fn from_rng<R: Rng>(rng: &mut R) -> Self {
        Self(rng.r#gen())
    }

    pub fn as_bytes(&self) -> &[u8; TOPIC_LENGTH] {
        &self.0
    }

    pub fn to_bytes(self) -> [u8; TOPIC_LENGTH] {
        self.0
    }
}

impl Default for Topic {
    fn default() -> Self {
        Self::new()
    }
}

impl From<[u8; TOPIC_LENGTH]> for Topic {
    fn from(topic: [u8; TOPIC_LENGTH]) -> Self {
        Self(topic)
    }
}

impl From<Topic> for [u8; TOPIC_LENGTH] {
    fn from(topic: Topic) -> Self {
        topic.0
    }
}

impl From<Hash> for Topic {
    fn from(value: Hash) -> Self {
        Self(*value.as_bytes())
    }
}

impl From<Topic> for Hash {
    fn from(topic: Topic) -> Self {
        Hash::from_bytes(topic.0)
    }
}

impl From<PublicKey> for Topic {
    fn from(value: PublicKey) -> Self {
        Self(*value.as_bytes())
    }
}

impl Display for Topic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl FromStr for Topic {
    type Err = TopicError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(hex::decode(value)?.as_slice())
    }
}

impl TryFrom<&[u8]> for Topic {
    type Error = TopicError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let value_len = value.len();

        let checked_value: [u8; TOPIC_LENGTH] = value
            .try_into()
            .map_err(|_| TopicError::InvalidLength(value_len, TOPIC_LENGTH))?;

        Ok(Self::from(checked_value))
    }
}

#[derive(Debug, Error)]
pub enum TopicError {
    /// Invalid number of bytes.
    #[error("invalid bytes length of {0}, expected {1} bytes")]
    InvalidLength(usize, usize),

    /// String contains invalid hexadecimal characters.
    #[error("invalid hex encoding in string")]
    InvalidHexEncoding(#[from] hex::FromHexError),
}
