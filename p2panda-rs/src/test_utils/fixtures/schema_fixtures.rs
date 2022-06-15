// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use rstest::fixture;

use crate::schema::{FieldType, Schema, SchemaId};
use crate::test_utils::constants::TEST_SCHEMA_ID;

/// Fixture which injects the default schema id into a test method. Default value can be
/// overridden at testing time by passing in a custom schema id string.
#[fixture]
pub fn schema(#[default(TEST_SCHEMA_ID)] schema_id: &str) -> SchemaId {
    SchemaId::new(schema_id).unwrap()
}

#[fixture]
pub fn schema_item(
    #[default(schema(TEST_SCHEMA_ID))] schema_id: SchemaId,
    #[default("test schema")] description: &str,
    #[default(vec![
        ("message", FieldType::String)
    ])]
    fields: Vec<(&str, FieldType)>,
) -> Schema {
    let mut fields_map = BTreeMap::new();
    for (field_name, field_type) in fields {
        fields_map.insert(field_name.to_owned(), field_type);
    }
    Schema::new_definition(schema_id, description.to_owned(), fields_map)
}

#[fixture]
pub fn default_schema(
    #[default(schema(TEST_SCHEMA_ID))] schema_id: SchemaId,
    #[default("test schema")] description: &str,
    #[default(vec![
        ("username", FieldType::String),
        ("height", FieldType::Float),
        ("age", FieldType::Int),
        ("is_admin", FieldType::Bool),
        ("profile_picture", FieldType::Relation(schema(TEST_SCHEMA_ID))),
        ("my_friends", FieldType::RelationList(schema(TEST_SCHEMA_ID)))
    ])]
    fields: Vec<(&str, FieldType)>,
) -> Schema {
    let mut fields_map = BTreeMap::new();
    for (field_name, field_type) in fields {
        fields_map.insert(field_name.to_owned(), field_type);
    }
    Schema::new_definition(schema_id, description.to_owned(), fields_map)
}
