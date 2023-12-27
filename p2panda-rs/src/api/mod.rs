// SPDX-License-Identifier: AGPL-3.0-or-later

//! Common validation and API methods following the p2panda specification.
mod error;
mod publish;
mod validation;

pub use error::{DomainError, ValidationError};
pub use publish::publish;
pub use validation::{
    validate_backlink, validate_plain_operation, validate_previous,
};
