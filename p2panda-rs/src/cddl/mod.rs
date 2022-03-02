// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods and structs to validate operations and the schemas they follow.
//!
//! This uses [`Concise Data Definition Language`] (CDDL) internally to verify CBOR data of p2panda
//! operations.
//!
//! [`Concise Data Definition Language`]: https://tools.ietf.org/html/rfc8610
mod error;
mod constants;
mod validation;
#[allow(clippy::module_inception)]
mod cddl;

pub use self::cddl::CDDLBuilder;
pub use error::CDDLValidationError;
pub use constants::OPERATION_FORMAT;
pub use validation::validate_cddl;
