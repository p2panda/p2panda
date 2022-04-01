// SPDX-License-Identifier: AGPL-3.0-or-later

//! Schemas describe the format of data used in operation fields.
mod error;
mod field_types;
#[allow(clippy::module_inception)]
mod schema;
mod schema_id;
pub mod system;

pub use error::{FieldTypeError, SchemaError, SchemaIdError};
pub use field_types::FieldType;
pub use schema::Schema;
pub use schema_id::{SchemaId, SchemaVersion};
