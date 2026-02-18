// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Display;
use std::hash::Hash as StdHash;

use rand::Rng;
use rand::rngs::OsRng;
use thiserror::Error;

use crate::{Hash, PublicKey};

const TOPIC_LENGTH: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, StdHash)]
pub struct Topic(pub(crate) [u8; TOPIC_LENGTH]);

impl Topic {
    pub fn new() -> Self {
        let mut rng = OsRng;
        Self::from_rng(&mut rng)
    }

    pub fn from_rng<R: Rng>(rng: &mut R) -> Self {
        Self(rng.r#gen())
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

#[derive(Error, Debug)]
pub enum TopicError {
    /// Invalid number of bytes.
    #[error("invalid bytes length of {0}, expected {1} bytes")]
    InvalidLength(usize, usize),
}
