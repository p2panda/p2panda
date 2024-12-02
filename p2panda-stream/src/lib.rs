// SPDX-License-Identifier: AGPL-3.0-or-later

//! Stream-based methods to process p2panda operations.
//!
//! After a messages has been received via gossip "live mode" or a sync session in `p2panda-net` we
//! usually need to apply additional tasks to process the received operation.
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
mod stream;
#[cfg(test)]
mod test_utils;

pub use stream::*;
