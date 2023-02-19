// SPDX-License-Identifier: AGPL-3.0-or-later

//! Schemas describe the format of data used in operation fields.
pub mod error;
mod field_types;
#[allow(clippy::module_inception)]
mod schema;
mod schema_id;
mod schema_name;
pub mod system;
pub mod validate;

pub use field_types::FieldType;
pub use schema::{FieldName, Schema};
pub use schema_id::{SchemaId, SchemaVersion};
pub use schema_name::SchemaName;
pub use system::SYSTEM_SCHEMAS;
