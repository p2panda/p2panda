// SPDX-License-Identifier: AGPL-3.0-or-later

//! System schemas are p2panda's built-in schema type.
//!
//! They are defined as part of the p2panda specification and may differ from application schemas
//! in how they are materialised.
use once_cell::sync::Lazy;

use crate::schema::Schema;

mod blob_piece;
mod error;
mod schema_definition;
mod schema_field_definition;
mod schema_views;

pub use error::SystemSchemaError;
pub use schema_views::{SchemaFieldView, SchemaView};

pub(super) use blob_piece::get_blob_piece;
pub(super) use schema_definition::get_schema_definition;
pub(super) use schema_field_definition::get_schema_field_definition;

/// A vector of all system schemas in this version of the library.
pub static SYSTEM_SCHEMAS: Lazy<Vec<&'static Schema>> = Lazy::new(|| {
    // Unwrap here because we know that these system schema versions exist
    vec![
        get_schema_definition(1).unwrap(),
        get_schema_field_definition(1).unwrap(),
        get_blob_piece(1).unwrap(),
    ]
});

#[cfg(test)]
mod test {
    use super::SYSTEM_SCHEMAS;

    #[test]
    fn test_static_system_schemas() {
        assert_eq!(SYSTEM_SCHEMAS.len(), 3);
    }
}
