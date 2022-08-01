// SPDX-License-Identifier: AGPL-3.0-or-later

//! Error types for validating operation fields against schemas.
use thiserror::Error;

/// Custom error types for validating raw operations with schemas.
#[derive(Error, Debug)]
pub enum ValidationError {
    /// Field with this name is required by schema.
    #[error("missing required field: '{0}' of type {1}")]
    MissingField(String, String),

    /// One or more fields which do not belong to the schema.
    #[error("unexpected fields found: {0}")]
    UnexpectedFields(String),

    /// Raw operation field did not match schema.
    #[error("field '{0}' does not match schema: {1}")]
    InvalidField(String, String),

    /// Field type and schema do not match.
    #[error("expected field name '{1}'")]
    InvalidName(String, String),

    /// Field type and schema do not match.
    #[error("invalid field type '{0}', expected '{1}'")]
    InvalidType(String, String),

    /// Field value is not correctly formatted.
    #[error("{0}")]
    InvalidValue(String),

    /// Field value is not in canonic format.
    #[error("non-canonic document view id, {0}")]
    InvalidDocumentViewId(String),
}

/// Custom error types for validating operations against `schema_field_definition_v1` schema.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum SchemaFieldDefinitionError {
    /// Operation contains wrong number of fields for this schema.
    #[error("unexpected number of operation fields in schema field definition")]
    UnexpectedFields,

    /// "name" field is missing.
    #[error("schema field definitions need a 'name' field")]
    NameMissing,

    /// "name" is not correctly formatted as per specification.
    #[error("'name' field in schema field definitions is wrongly formatted")]
    NameInvalid,

    /// "name" field type is not a "str".
    #[error("'name' field in schema field definitions needs to be of type 'str'")]
    NameWrongType,

    /// "type" field is missing.
    #[error("schema field definitions need a 'type' field")]
    TypeMissing,

    /// "type" is not correctly formatted as per specification.
    #[error("'type' field in schema field definitions is wrongly formatted")]
    TypeInvalid,

    /// "type" field type is not a "str".
    #[error("'type' field in schema field definitions needs to be of type 'str'")]
    TypeWrongType,
}

/// Custom error types for validating operations against `schema_field_definition_v1` schema.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum SchemaDefinitionError {
    /// Operation contains wrong number of fields for this schema.
    #[error("unexpected number of operation fields in schema definition")]
    UnexpectedFields,

    /// "name" field is missing.
    #[error("schema field definitions need a 'name' field")]
    NameMissing,

    /// "name" is not correctly formatted as per specification.
    #[error("'name' field in schema field definitions is wrongly formatted")]
    NameInvalid,

    /// "name" field type is not a "str".
    #[error("'name' field in schema field definitions needs to be of type 'str'")]
    NameWrongType,

    /// "description" field is missing.
    #[error("schema field definitions need a 'description' field")]
    DescriptionMissing,

    /// "description" is not correctly formatted as per specification.
    #[error("'description' field in schema field definitions is wrongly formatted")]
    DescriptionInvalid,

    /// "description" field type is not a "str".
    #[error("'description' field in schema field definitions needs to be of type 'str'")]
    DescriptionWrongType,

    /// "fields" field is missing.
    #[error("schema field definitions need a 'fields' field")]
    FieldsMissing,

    /// "fields" is not correctly formatted as per specification.
    #[error("'fields' field in schema field definitions is wrongly formatted")]
    FieldsInvalid,

    /// "fields" field type is not a "str".
    #[error("'fields' field in schema field definitions needs to be of type 'str'")]
    FieldsWrongType,
}
