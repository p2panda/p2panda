// SPDX-License-Identifier: AGPL-3.0-or-later

//! Common validation and API methods following the p2panda specification.
mod errors;
pub mod helpers;
mod next_args;
mod publish;
pub mod validation;

pub use errors::{DomainError, ValidationError};
pub use next_args::next_args;
pub use publish::publish;
