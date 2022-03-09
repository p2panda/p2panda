// SPDX-License-Identifier: AGPL-3.0-or-later

//! Schemas describe the format of data used in operation fields.
mod error;
#[allow(clippy::module_inception)]
mod schema;
mod schema_id;
pub mod system;

pub use error::{SchemaError, SchemaIdError};
pub use schema_id::{SchemaFieldV1, SchemaId, SchemaV1};
