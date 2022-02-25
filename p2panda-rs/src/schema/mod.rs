// SPDX-License-Identifier: AGPL-3.0-or-later

//! Validation for operations and definitions of system schemas.
//!
//! This uses [`Concise Data Definition Language`] (CDDL) internally to verify CBOR data of p2panda
//! operations.
//!
//! [`Concise Data Definition Language`]: https://tools.ietf.org/html/rfc8610

#[allow(clippy::module_inception)]
mod cddl_builder;
mod error;
mod operation;
mod schema_id;
mod system_schema;
#[cfg(not(target_arch = "wasm32"))]
mod validation;

pub use cddl_builder::CDDLBuilder;
pub use error::{SchemaValidationError, SystemSchemaError};
pub use operation::OPERATION_SCHEMA;
pub use schema_id::SchemaId;
#[cfg(not(target_arch = "wasm32"))]
pub use validation::validate_schema;
