// SPDX-License-Identifier: AGPL-&3.0-or-later

use std::str::FromStr;

use lazy_static::lazy_static;
use regex::Regex;

use crate::next::operation::plain::{PlainFields, PlainValue};
use crate::next::schema::validate::error::SchemaFieldDefinitionError;
use crate::next::schema::SchemaId;

/// Checks "name" field in a schema field definition operation.
///
/// 1. It must be at most 64 characters long
/// 2. It begins with a letter
/// 3. It uses only alphanumeric characters, digits and the underscore character
fn validate_name(value: &str) -> bool {
    lazy_static! {
        // Unwrap as we checked the regular expression for correctness
        static ref NAME_REGEX: Regex = Regex::new("^[A-Za-z]{1}[A-Za-z0-9_]{0,63}$").unwrap();
    }

    NAME_REGEX.is_match(value)
}

/// Checks "type" field in a schema field definition operation.
///
/// 1. It must be one of: bool, int, float, str, relation, pinned_relation, relation_list,
///    pinned_relation_list
/// 2. Relations need to specify a valid and canonical schema id
fn validate_type(value: &str) -> bool {
    match value {
        "bool" | "int" | "float" | "str" => true,
        relation => validate_relation_type(relation),
    }
}

/// Checks format for "type" fields which specify a relation.
///
/// 1. The first section is the name, which must have 1-64 characters, must start with a letter and
///    must contain only alphanumeric characters and underscores
/// 2. The remaining sections are the document view id, represented as alphabetically sorted
///    hex-encoded operation ids, separated by underscores
fn validate_relation_type(value: &str) -> bool {
    // Parse relation value
    lazy_static! {
        static ref RELATION_REGEX: Regex = {
            let schema_id = "[A-Za-z]{1}[A-Za-z0-9_]{0,63}_([0-9A-Za-z]{68})(_[0-9A-Za-z]{68}*";

            // Unwrap as we checked the regular expression for correctness
            Regex::new(&format!(r"(\w+)\(({})\)", schema_id)).unwrap()
        };
    }

    let groups = RELATION_REGEX.captures(value);
    if groups.is_none() {
        return false;
    }

    let relation_type_str = groups
        .as_ref()
        // Unwrap now as we checked if its `None` before
        .unwrap()
        .get(2)
        .map(|group_match| group_match.as_str());

    let schema_id_str = groups
        .as_ref()
        // Unwrap now as we checked if its `None` before
        .unwrap()
        .get(2)
        .map(|group_match| group_match.as_str());

    // Check if relation type is known
    let is_valid_relation_type = match relation_type_str {
        Some(type_str) => {
            matches!(
                type_str,
                "relation" | "pinned_relation" | "relation_list" | "pinned_relation_list"
            )
        }
        None => false,
    };

    // Check if schema id is correctly (valid hashes) and canonically formatted (no duplicates,
    // sorted operation ids)
    let is_valid_schema_id = match schema_id_str {
        Some(str) => {
            return SchemaId::from_str(str).is_ok();
        }
        None => false,
    };

    is_valid_relation_type && is_valid_schema_id
}

/// Validate formatting for operations following `schema_field_definition_v1` system schemas.
///
/// These operations contain a "name" and "type" field with each have special limitations defined
/// by the p2panda specification.
pub fn validate_schema_field_definition_v1_fields(
    fields: &PlainFields,
) -> Result<(), SchemaFieldDefinitionError> {
    // Check that there are only two fields given
    if fields.len() != 2 {
        return Err(SchemaFieldDefinitionError::UnexpectedFields);
    }

    // Check "name" field
    let field_name = fields
        .get("name")
        .ok_or(SchemaFieldDefinitionError::NameMissing)?;

    match field_name {
        PlainValue::StringOrRelation(value) => {
            if validate_name(value) {
                Ok(())
            } else {
                Err(SchemaFieldDefinitionError::NameInvalid)
            }
        }
        _ => Err(SchemaFieldDefinitionError::NameWrongType),
    }?;

    // Check "type" field
    let field_type = fields
        .get("type")
        .ok_or(SchemaFieldDefinitionError::TypeMissing)?;

    match field_type {
        PlainValue::StringOrRelation(value) => {
            if validate_type(value) {
                Ok(())
            } else {
                Err(SchemaFieldDefinitionError::TypeInvalid)
            }
        }
        _ => Err(SchemaFieldDefinitionError::TypeWrongType),
    }?;

    Ok(())
}
