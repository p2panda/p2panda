// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods and structs to generate CDDL for CBOR validation.
//!
//! All operations in p2panda are encoded via CBOR and can be checked against the right format via
//! CDDL.
//!
//! Read more about CDDL: https://tools.ietf.org/html/rfc8610
mod builder;
mod constants;
mod error;
mod validation;

pub use builder::CddlBuilder;
pub use constants::OPERATION_FORMAT;
pub use error::CddlValidationError;
pub use validation::validate_cddl;
