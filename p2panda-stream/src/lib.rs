// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(doctest, doc=include_str!("../README.md"))]

//! Stream-based methods to conveniently handle p2panda operations.
//!
//! `p2panda-stream` is a collection of various methods which help to decode, validate, order,
//! prune or store p2panda operations. More methods are planned in the future.
//!
//! With the stream-based design it is easy to "stack" these methods on top of each other,
//! depending on the requirements of the application (or each "topic" data stream). Like this a
//! user can decide if they want to persist data or keep it "ephemeral", apply automatic pruning
//! techniques for outdated operations etc.
mod macros;
pub mod operation;
mod ordering;
mod stream;
#[cfg(test)]
mod test_utils;

pub use ordering::*;
pub use stream::*;
