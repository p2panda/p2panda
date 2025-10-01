// SPDX-License-Identifier: MIT OR Apache-2.0

mod processor;
#[allow(clippy::module_inception)]
mod spaces;

pub use processor::Spaces;
pub use spaces::{SpacesBuilder, SpacesError};
