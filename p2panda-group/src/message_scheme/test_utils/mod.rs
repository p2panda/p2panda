// SPDX-License-Identifier: MIT OR Apache-2.0

pub mod dcgka;
pub mod network;

use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::traits::OperationId;

pub type MemberId = usize;

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
