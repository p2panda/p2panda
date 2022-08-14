// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use lazy_static::lazy_static;

use crate::schema::error::SchemaIdError;
use crate::schema::{FieldType, Schema, SchemaId};

const DESCRIPTION: &str = "Publish data schemas for your application.";

lazy_static! {
    pub static ref SCHEMA_DEFINITION_V1: Schema = {
        let mut fields = BTreeMap::new();

        fields.insert("name".to_string(), FieldType::String);
        fields.insert("description".to_string(), FieldType::String);

        fields.insert(
            "fields".to_string(),
            FieldType::PinnedRelationList(SchemaId::SchemaFieldDefinition(1)),
        );

        Schema {
            id: SchemaId::SchemaDefinition(1),
            description: DESCRIPTION.to_owned(),
            fields,
        }
    };
}

/// Returns the `schema_definition` system schema with a given version.
pub fn get_schema_definition(version: u8) -> Result<&'static Schema, SchemaIdError> {
    match version {
        1 => Ok(&SCHEMA_DEFINITION_V1),
        _ => Err(SchemaIdError::UnknownSystemSchema(
            SchemaId::SchemaDefinition(version).to_string(),
        )),
    }
}
