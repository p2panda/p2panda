// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest::fixture;

use crate::schema::SchemaId;
use crate::test_utils::constants::SCHEMA_ID;

/// Fixture which injects the default schema id into a test method. Default value can be
/// overridden at testing time by passing in a custom schema id string.
#[fixture]
pub fn schema(#[default(SCHEMA_ID)] schema_id: &str) -> SchemaId {
    SchemaId::new(schema_id).unwrap()
}
