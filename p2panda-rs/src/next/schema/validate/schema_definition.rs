// SPDX-License-Identifier: AGPL-&3.0-or-later

use lazy_static::lazy_static;
use regex::Regex;

use crate::next::operation::plain::{PlainFields, PlainValue};
use crate::next::schema::validate::error::SchemaDefinitionError;

/// Checks "name" field of operations with "schema_definition_v1" schema id.
///
/// 1. It must be at most 64 characters long
/// 2. It begins with a letter
/// 3. It uses only alphanumeric characters, digits and the underscore character
/// 4. It doesn't end with an underscore
fn validate_name(value: &str) -> bool {
    lazy_static! {
        // Unwrap as we checked the regular expression for correctness
        static ref NAME_REGEX: Regex = Regex::new(
            "^[A-Za-z]{1}[A-Za-z0-9_]{0,62}[A-Za-z0-9]{1}$
        ").unwrap();
    }

    NAME_REGEX.is_match(value)
}

/// Checks "description" field of operations with "schema_definition_v1" schema id.
///
/// 1. It consists of unicode characters
/// 2. ... and must be at most 256 characters long
fn validate_description(value: &str) -> bool {
    value.chars().count() <= 256
}

/// Checks "fields" field of operations with "schema_definition_v1" schema id.
///
/// 1. A schema must have at most 1024 fields
fn validate_fields(value: &Vec<Vec<String>>) -> bool {
    value.len() <= 1024
}

/// Validate formatting for operations following `schema_definition_v1` system schemas.
///
/// These operations contain a "name", "description" and "fields" field with each have special
/// limitations defined by the p2panda specification.
pub fn validate_schema_definition_v1_fields(
    fields: &PlainFields,
) -> Result<(), SchemaDefinitionError> {
    // Check that there are only three fields given
    if fields.len() != 3 {
        return Err(SchemaDefinitionError::UnexpectedFields);
    }

    // Check "name" field
    let schema_name = fields
        .get("name")
        .ok_or(SchemaDefinitionError::NameMissing)?;

    if let PlainValue::StringOrRelation(value) = schema_name {
        if validate_name(value) {
            Ok(())
        } else {
            Err(SchemaDefinitionError::NameInvalid)
        }
    } else {
        Err(SchemaDefinitionError::NameWrongType)
    }?;

    // Check "description" field
    let schema_description = fields
        .get("description")
        .ok_or(SchemaDefinitionError::DescriptionMissing)?;

    match schema_description {
        PlainValue::StringOrRelation(value) => {
            if validate_description(value) {
                Ok(())
            } else {
                Err(SchemaDefinitionError::DescriptionInvalid)
            }
        }
        _ => Err(SchemaDefinitionError::DescriptionWrongType),
    }?;

    // Check "fields" field
    let schema_fields = fields
        .get("fields")
        .ok_or(SchemaDefinitionError::FieldsMissing)?;

    match schema_fields {
        PlainValue::PinnedRelationList(value) => {
            if validate_fields(value) {
                Ok(())
            } else {
                Err(SchemaDefinitionError::FieldsInvalid)
            }
        }
        _ => Err(SchemaDefinitionError::FieldsWrongType),
    }?;

    Ok(())
}
