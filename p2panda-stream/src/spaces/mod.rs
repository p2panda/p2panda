// SPDX-License-Identifier: MIT OR Apache-2.0

//! Types and methods for processing spaces messages for group encryption.
mod args;
mod processor;

pub use args::SpacesProcessorArgs;
pub use processor::{Spaces, SpacesManager, SpacesManagerError, SpacesResult};
