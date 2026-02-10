// SPDX-License-Identifier: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};

pub type Header = p2panda_core::Header<Extensions>;

pub type Operation = p2panda_core::Operation<Extensions>;

// TODO: Make sure encoding is canonical over map keys (sort it before serializing).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extensions {
    version: u64,
}
