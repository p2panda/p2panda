// SPDX-License-Identifier: MIT OR Apache-2.0

mod args;
mod processor;

pub use args::LogPruneArgs;
pub use processor::{LogPrune, LogPruneError, LogPruneResult};
