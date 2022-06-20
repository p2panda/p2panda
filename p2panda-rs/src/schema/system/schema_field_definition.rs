// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use lazy_static::lazy_static;

use crate::schema::{FieldType, Schema, SchemaId, SchemaIdError};

const DESCRIPTION: &str = "Define fields for application data schemas.";

lazy_static! {
    pub static ref SCHEMA_FIELD_DEFINITION_V1: Schema = {
        let mut fields = BTreeMap::new();
        fields.insert("name".to_string(), FieldType::String);
        fields.insert("type".to_string(), FieldType::String);
        Schema {
            id: SchemaId::SchemaFieldDefinition(1),
            description: DESCRIPTION.to_owned(),
            fields,
        }
    };
}

/// Returns the `schema_field_definition` system schema with a given version.
pub fn get_schema_field_definition(version: u8) -> Result<&'static Schema, SchemaIdError> {
    match version {
        1 => Ok(&SCHEMA_FIELD_DEFINITION_V1),
        _ => Err(SchemaIdError::UnknownSystemSchema(
            SchemaId::SchemaFieldDefinition(version).as_str(),
        )),
    }
}
