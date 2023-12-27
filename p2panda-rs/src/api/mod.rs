// SPDX-License-Identifier: AGPL-3.0-or-later

//! Common validation and API methods following the p2panda specification.
mod errors;
mod publish;
mod validation;

pub use errors::{DomainError, ValidationError};
pub use publish::publish;
pub use validation::{validate_backlink, validate_previous, validate_header_extensions, validate_plain_operation};
