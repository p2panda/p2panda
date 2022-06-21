// SPDX-License-Identifier: AGPL-3.0-or-later

//! System schemas are p2panda's built-in schema type.
//!
//! They are defined as part of the p2panda specification and may differ from application schemas
//! in how they are materialised.

use lazy_static::lazy_static;

mod error;
mod schema_definition;
mod schema_field_definition;
mod schema_views;

pub use error::SystemSchemaError;
pub use schema_views::{SchemaFieldView, SchemaView};

pub(super) use schema_definition::get_schema_definition;
pub(super) use schema_field_definition::get_schema_field_definition;

use crate::schema::Schema;

lazy_static! {
    /// A vector of all system schemas in this version of the library.
    pub static ref SYSTEM_SCHEMAS: Vec<&'static Schema> = vec![
        get_schema_definition(1).unwrap(),
        get_schema_field_definition(1).unwrap(),
    ];
}

#[cfg(test)]
mod test {
    use super::SYSTEM_SCHEMAS;

    #[test]
    fn test_static_system_schemas() {
        assert_eq!(SYSTEM_SCHEMAS.len(), 2);
    }
}
