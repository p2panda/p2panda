// SPDX-License-Identifier: MIT OR Apache-2.0

use std::hash::Hash as StdHash;

use p2panda_core::hash::{HASH_LEN, Hash};
use p2panda_core::{PruneFlag, Topic};
use serde::{Deserialize, Serialize};

pub type Header = p2panda_core::Header<Extensions>;

pub type Operation = p2panda_core::Operation<Extensions>;

/// Versioning for internal extensions format.
pub(crate) const VERSION: u64 = 1;

// TODO: Make sure encoding is canonical over map keys (sort it before serializing).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extensions {
    #[serde(
        skip_serializing_if = "PruneFlag::is_not_set",
        default = "PruneFlag::default"
    )]
    pub prune_flag: PruneFlag,
    pub log_id: LogId,
    pub version: u64,
}

impl Extensions {
    pub(crate) fn from_topic(topic: Topic) -> Self {
        Self {
            log_id: LogId::from_topic(topic),
            prune_flag: PruneFlag::default(),
            version: VERSION,
        }
    }

    pub(crate) fn prune_flag(mut self, prune_flag: bool) -> Self {
        self.prune_flag = prune_flag.into();
        self
    }
}

#[derive(Clone, Copy, Debug, Ord, PartialOrd, PartialEq, Eq, StdHash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LogId(Hash);

impl LogId {
    /// Derive log id from a topic.
    ///
    /// Since topics are randomly generated we get the guarantee that every log and thus operation
    /// will be uniquely identifiable.
    ///
    /// To keep topic itself private we derive it with a BLAKE3 digest.
    pub fn from_topic(topic: Topic) -> Self {
        LogId(Hash::new(topic.as_bytes()))
    }

    pub fn as_bytes(&self) -> &[u8; HASH_LEN] {
        self.0.as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::Topic;

    use super::LogId;

    #[test]
    fn derive_from_topic() {
        let topic = Topic::new();
        let log_id = LogId::from_topic(topic);
        assert_ne!(topic.as_bytes(), log_id.as_bytes());
    }
}
