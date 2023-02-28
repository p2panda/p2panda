// SPDX-License-Identifier: AGPL-3.0-or-later

use once_cell::sync::Lazy;

use crate::schema::error::SchemaIdError;
use crate::schema::{FieldType, Schema, SchemaDescription, SchemaFields, SchemaId};

const DESCRIPTION: &str = "Publish data schemas for your application.";

pub static SCHEMA_DEFINITION_V1: Lazy<Schema> = Lazy::new(|| {
    let fields = SchemaFields::new(&[
        ("name", FieldType::String),
        ("description", FieldType::String),
        (
            "fields",
            FieldType::PinnedRelationList(SchemaId::SchemaFieldDefinition(1)),
        ),
    ])
    // Unwrap as we know the fields are valid.
    .unwrap();

    // We can unwrap here as we know the schema definition is valid.
    let description = SchemaDescription::new(DESCRIPTION).unwrap();

    Schema {
        id: SchemaId::SchemaDefinition(1),
        description,
        fields,
    }
});

/// Returns the `schema_definition` system schema with a given version.
pub fn get_schema_definition(version: u8) -> Result<&'static Schema, SchemaIdError> {
    match version {
        1 => Ok(&SCHEMA_DEFINITION_V1),
        _ => Err(SchemaIdError::UnknownSystemSchema(
            SchemaId::SchemaDefinition(version).to_string(),
        )),
    }
}
