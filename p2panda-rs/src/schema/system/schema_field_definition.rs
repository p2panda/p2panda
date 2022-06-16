// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use crate::schema::{FieldType, Schema, SchemaId, SchemaIdError};

const DESCRIPTION: &str = "define fields for application data schemas";

/// Returns the `schema_field_definition` system schema with a given version.
pub fn get_schema_field_definition(version: u8) -> Result<Schema, SchemaIdError> {
    match version {
        1 => {
            let mut fields = BTreeMap::new();
            fields.insert("name".to_string(), FieldType::String);
            fields.insert("type".to_string(), FieldType::String);
            Ok(Schema {
                id: SchemaId::SchemaFieldDefinition(version),
                description: DESCRIPTION.to_owned(),
                fields,
            })
        }
        _ => Err(SchemaIdError::UnknownSystemSchema(
            SchemaId::SchemaFieldDefinition(version).as_str(),
        )),
    }
}
