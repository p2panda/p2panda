// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use crate::schema::{FieldType, Schema, SchemaId, SchemaIdError};

const DESCRIPTION: &str = "Publish data schemas for your application.";

/// Returns the `schema_definition` system schema with a given version.
pub fn get_schema_definition(version: u8) -> Result<Schema, SchemaIdError> {
    match version {
        1 => {
            let mut fields = BTreeMap::new();
            fields.insert("name".to_string(), FieldType::String);
            fields.insert("description".to_string(), FieldType::String);
            fields.insert(
                "fields".to_string(),
                FieldType::PinnedRelationList(SchemaId::SchemaFieldDefinition(1)),
            );
            Ok(Schema {
                id: SchemaId::SchemaDefinition(version),
                description: DESCRIPTION.to_owned(),
                fields,
            })
        }
        _ => Err(SchemaIdError::UnknownSystemSchema(
            SchemaId::SchemaDefinition(version).as_str(),
        )),
    }
}
