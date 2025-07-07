// SPDX-License-Identifier: MIT OR Apache-2.0

//! Internal helpers for writing (fuzz-) tests against `p2panda-encryption`.
use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::traits::OperationId;

// Re-export private types for fuzz tests when `test_utils` feature flag is enabled.
pub mod crypto {
    pub use crate::crypto::x25519::SecretKey;
}

pub mod data_scheme {
    pub use crate::data_scheme::test_utils::*;
}

pub mod message_scheme {
    pub use crate::message_scheme::test_utils::*;
}

/// Simple member id for tests.
pub type MemberId = usize;

/// Simple message id for tests with monotonically incrementing sequence numbers per peer.
///
/// This contains the sender again as we need unique message ids and just using the sequence number
/// would not be sufficient.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId {
    pub sender: MemberId,
    pub seq: usize,
}

impl MessageId {
    pub fn new(my_id: MemberId) -> Self {
        Self {
            sender: my_id,
            seq: 0,
        }
    }

    pub fn inc(&self) -> Self {
        Self {
            sender: self.sender,
            seq: self.seq + 1,
        }
    }
}

impl Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[id={}, seq={}]", self.sender, self.seq)
    }
}

impl OperationId for MessageId {}
