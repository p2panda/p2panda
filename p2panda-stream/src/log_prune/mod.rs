// SPDX-License-Identifier: MIT OR Apache-2.0

//! Coordinates pruning of a log's prefix based on a prune flag.
//!
//! When a prune flag is set in an operation, an author signals to others that all operations can be
//! deleted (including payloads) in that log _before_ it.
//!
//! ```text
//! Log of Author A with six Operations:
//!
//! [ 0 ] <-- can be removed
//! [ 1 ] <-- can be removed
//! [ 2 ] <-- can be removed
//! [ 3 ] <-- prune flag = true
//! [ 4 ]
//! [ 5 ]
//! ...
//! ```
mod args;
mod processor;

pub use args::LogPruneArgs;
pub use processor::{LogPrune, LogPruneError, LogPruneResult};
