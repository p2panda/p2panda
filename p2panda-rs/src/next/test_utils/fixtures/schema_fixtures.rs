// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest::fixture;

use crate::next::schema::{FieldType, Schema, SchemaId};
use crate::test_utils::constants::SCHEMA_ID;

/// Fixture which injects the default schema id into a test method.
///
/// Default value can be overridden at testing time by passing in a custom schema id string.
#[fixture]
pub fn schema_id(#[default(SCHEMA_ID)] schema_id_str: &str) -> SchemaId {
    SchemaId::new(schema_id_str).unwrap()
}

/// Fixture which injects schema struct into a test method.
///
/// Default value can be overridden at testing time by passing in a custom schema id string.
#[fixture]
pub fn schema(
    #[default(vec![("address", FieldType::String)])] fields: Vec<(&str, FieldType)>,
    #[default(schema_id(SCHEMA_ID))] schema_id: SchemaId,
    #[default("Test schema")] description: &str,
) -> Schema {
    Schema::new(&schema_id, description, fields).unwrap()
}
