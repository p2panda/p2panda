// SPDX-License-Identifier: AGPL-3.0-or-later

mod domain;
mod errors;
pub mod validation;

pub use domain::{next_args, publish};
pub use errors::{DomainError, ValidationError};