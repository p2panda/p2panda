// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest::fixture;

use crate::operation_v2::OperationValue;
use crate::schema::{FieldName, FieldType, Schema, SchemaId, SchemaName};
use crate::test_utils::constants::{self, SCHEMA_ID};
use crate::test_utils::fixtures::operation_value;

/// Returns constant schema id.
#[fixture]
pub fn schema_name(#[default("venue")] schema_name_str: &str) -> SchemaName {
    SchemaName::new(schema_name_str).expect("Valid schema name")
}

/// Returns constant schema id.
#[fixture]
pub fn schema_id(#[default(SCHEMA_ID)] schema_id_str: &str) -> SchemaId {
    SchemaId::new(schema_id_str).unwrap()
}

/// Generates a name of a schema field.
#[fixture]
pub fn schema_field_name(#[default("venue")] name: &str) -> FieldName {
    name.to_owned()
}

/// Derives a schema field type from operation value.
#[fixture]
pub fn schema_field_type(
    #[from(operation_value)] value: OperationValue,
    #[from(schema_id)] schema_id: SchemaId,
) -> FieldType {
    match value {
        OperationValue::Boolean(_) => FieldType::Boolean,
        OperationValue::Bytes(_) => FieldType::Bytes,
        OperationValue::Integer(_) => FieldType::Integer,
        OperationValue::Float(_) => FieldType::Float,
        OperationValue::String(_) => FieldType::String,
        OperationValue::Relation(_) => FieldType::Relation(schema_id),
        OperationValue::RelationList(_) => FieldType::RelationList(schema_id),
        OperationValue::PinnedRelation(_) => FieldType::PinnedRelation(schema_id),
        OperationValue::PinnedRelationList(_) => FieldType::PinnedRelationList(schema_id),
    }
}

/// Generates field which can be used to define a schema.
#[fixture]
pub fn schema_field(
    #[from(schema_field_name)] name: FieldName,
    #[from(schema_field_type)] value: FieldType,
) -> (FieldName, FieldType) {
    (name, value)
}

/// Generates schema fields from an operation. Sets schema ids of all relations to constant document
/// view id.
#[fixture]
pub fn schema_fields(
    #[default(constants::test_fields())] operation_fields_vec: Vec<(&str, OperationValue)>,
    #[from(schema_id)] schema_id: SchemaId,
) -> Vec<(FieldName, FieldType)> {
    let mut fields = Vec::new();

    // Derive schema fields from operation
    for field in operation_fields_vec {
        let field_name = field.0.to_owned();
        let field_type = schema_field_type(field.1, schema_id.clone());
        fields.push((field_name, field_type));
    }

    fields
}

/// Generates schema struct with default fields.
#[fixture]
pub fn schema(
    #[from(schema_fields)] fields: Vec<(FieldName, FieldType)>,
    #[default(schema_id(SCHEMA_ID))] schema_id: SchemaId,
    #[default("Test schema")] description: &str,
) -> Schema {
    let fields: Vec<(&str, FieldType)> = fields
        .iter()
        .map(|(name, field_type)| (name.as_str(), field_type.to_owned()))
        .collect();
    Schema::new(&schema_id, description, &fields).unwrap()
}
