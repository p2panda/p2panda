// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use once_cell::sync::Lazy;

use crate::schema::error::SchemaIdError;
use crate::schema::{FieldType, Schema, SchemaId, SchemaDescription};

const DESCRIPTION: &str = "Define fields for application data schemas.";

pub static SCHEMA_FIELD_DEFINITION_V1: Lazy<Schema> = Lazy::new(|| {
    let mut fields = BTreeMap::new();

    fields.insert("name".to_string(), FieldType::String);
    fields.insert("type".to_string(), FieldType::String);

    // We can unwrap here as we know the schema definition is valid.
    let description = SchemaDescription::new(DESCRIPTION).unwrap();

    Schema {
        id: SchemaId::SchemaFieldDefinition(1),
        description,
        fields,
    }
});

/// Returns the `schema_field_definition` system schema with a given version.
pub fn get_schema_field_definition(version: u8) -> Result<&'static Schema, SchemaIdError> {
    match version {
        1 => Ok(&SCHEMA_FIELD_DEFINITION_V1),
        _ => Err(SchemaIdError::UnknownSystemSchema(
            SchemaId::SchemaFieldDefinition(version).to_string(),
        )),
    }
}
