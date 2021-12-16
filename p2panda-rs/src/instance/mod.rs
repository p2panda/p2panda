// SPDX-License-Identifier: AGPL-3.0-or-later

//! Types and methods for deriving and maintaining `Instances`.
mod error;
#[allow(clippy::module_inception)]
mod instance;

pub use error::InstanceError;
pub use instance::Instance;
