// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods and structs to generate CDDL for CBOR validation.
//!
//! All operations in p2panda are encoded via CBOR and can be checked against the right format via
//! CDDL.
//!
//! Read more about CDDL: https://tools.ietf.org/html/rfc8610
mod definitions;
mod error;
mod generator;
mod validation;

pub use definitions::{OPERATION_FORMAT, SCHEMA_FIELD_V1_FORMAT, SCHEMA_V1_FORMAT};
pub use error::CddlValidationError;
<<<<<<< HEAD
<<<<<<< HEAD
pub use generator::generate_cddl_definition;
=======
>>>>>>> Complete refactor of CDDL generation code
=======
pub use generator::generate_cddl_definition;
>>>>>>> Generate CDDL definition from Schema struct
pub use validation::validate_cbor;
