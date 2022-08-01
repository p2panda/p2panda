// SPDX-License-Identifier: AGPL-3.0-or-later

//! Error types for creating schema instances or checking operations against them.
use thiserror::Error;

use crate::next::schema::SchemaId;

/// Custom errors related to `SchemaId`.
#[derive(Error, Debug)]
pub enum SchemaIdError {
    /// Encountered a malformed schema id.
    #[error("malformed schema id `{0}`: {1}")]
    MalformedSchemaId(String, String),

    /// Application schema ids must start with the schema's name.
    #[error("application schema id is missing a name: {0}")]
    MissingApplicationSchemaName(String),

    /// Invalid system schema id.
    #[error("unsupported system schema: {0}")]
    UnknownSystemSchema(String),

    /// Invalid hash in schema id.
    #[error("encountered invalid hash while parsing application schema id: {0}")]
    HashError(#[from] crate::next::hash::error::HashError),

    /// Handle errors from validating document view ids.
    #[error("encountered invalid document view id while parsing application schema id: {0}")]
    DocumentViewIdError(#[from] crate::next::document::error::DocumentViewIdError),

    /// Handle errors from validating operation ids.
    #[error("encountered invalid hash while parsing application schema id: {0}")]
    OperationIdError(#[from] crate::next::operation::error::OperationIdError),
}

/// Custom errors related to `Schema`.
#[derive(Error, Debug)]
pub enum SchemaError {
    /// Invalid fields in schema.
    #[error("invalid fields found for this schema")]
    InvalidFields,

    /// Use static definitions of system schemas instead of defining them dynamically.
    #[error("dynamic redefinition of system schema {0}, use `Schema::get_system` instead")]
    DynamicSystemSchema(SchemaId),

    /// Schemas must have valid schema ids.
    #[error(transparent)]
    SchemaIdError(#[from] SchemaIdError),
}

/// Custom error types for field types.
#[derive(Error, Debug)]
pub enum FieldTypeError {
    /// Invalid field type found.
    #[error("invalid field type '{0}'")]
    InvalidFieldType(String),

    /// Schema ids referenced by relation field types need to be valid.
    #[error(transparent)]
    RelationSchemaReference(#[from] SchemaIdError),
}

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
