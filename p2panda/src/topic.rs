// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Display;
use std::hash::Hash as StdHash;

use p2panda_core::{Hash, PublicKey};
use rand::rand_core::UnwrapErr;
use rand::rngs::SysRng;
use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, StdHash, Serialize, Deserialize)]
pub struct Topic([u8; 32]);

impl Topic {
    pub fn new() -> Self {
        let mut rng = UnwrapErr(SysRng);
        Self::from_rng(&mut rng)
    }

    pub fn from_rng<R: Rng>(rng: &mut R) -> Self {
        Self(rng.random())
    }
}

impl Default for Topic {
    fn default() -> Self {
        Self::new()
    }
}

impl From<[u8; 32]> for Topic {
    fn from(topic: [u8; 32]) -> Self {
        Self(topic)
    }
}

impl From<Topic> for [u8; 32] {
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
