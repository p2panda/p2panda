// SPDX-License-Identifier: AGPL-3.0-or-later

//! System schemas are p2panda's built-in schema type.
//!
//! They are defined as part of the p2panda specification and may differ from application schemas
//! in how they are materialised.
mod error;
mod schema_definition;
mod schema_field_definition;
mod schema_views;

pub use error::SystemSchemaError;
pub use schema_views::{SchemaFieldView, SchemaView};

use self::schema_definition::get_schema_definition;
use self::schema_field_definition::get_schema_field_definition;

use super::{Schema, SchemaId, SchemaIdError};

/// Return the schema struct for a system schema id.
///
/// Returns an error if this library version doesn't support the given system schema or this
/// particular version.
pub fn get_system_schema(schema_id: SchemaId) -> Result<Schema, SchemaIdError> {
    match schema_id {
        SchemaId::SchemaDefinition(version) => get_schema_definition(version),
        SchemaId::SchemaFieldDefinition(version) => get_schema_field_definition(version),
        _ => Err(SchemaIdError::UnknownSystemSchema(schema_id.as_str())),
    }
}

#[cfg(test)]
mod test {
    use crate::document::DocumentViewId;
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::document_view_id;
    use rstest::rstest;

    use super::get_system_schema;

    #[test]
    fn test_all_system_schemas() {
        let schema_definition = get_system_schema(SchemaId::SchemaDefinition(1)).unwrap();
        assert_eq!(
            schema_definition.to_string(),
            "<Schema schema_definition_v1>"
        );

        let schema_field_definition =
            get_system_schema(SchemaId::SchemaFieldDefinition(1)).unwrap();
        assert_eq!(
            schema_field_definition.to_string(),
            "<Schema schema_field_definition_v1>"
        );
    }

    #[rstest]
    fn test_error_application_schema(document_view_id: DocumentViewId) {
        let schema = get_system_schema(SchemaId::Application(
            "events".to_string(),
            document_view_id,
        ));
        assert!(schema.is_err())
    }
}
