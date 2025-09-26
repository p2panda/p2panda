// SPDX-License-Identifier: MIT OR Apache-2.0

use std::hash::Hash as StdHash;

pub trait OperationId: Clone + Copy + PartialEq + Eq + StdHash {}
