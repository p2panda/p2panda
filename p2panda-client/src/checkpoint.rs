// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use p2panda_core::{Hash, PublicKey};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Checkpoint(HashMap<PublicKey, Vec<Hash>>);

impl Checkpoint {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn add(&mut self, public_key: &PublicKey, log_heights: &[Hash]) {
        self.0
            .entry(*public_key)
            .and_modify(|value| value.append(&mut log_heights.to_vec()))
            .or_insert(log_heights.to_vec());
    }

    pub fn is_from_beginning(&self) -> bool {
        self.0.is_empty()
    }
}
