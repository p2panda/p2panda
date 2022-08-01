// SPDX-License-Identifier: AGPL-&3.0-or-later

//! Various methods to validate an operation against a schema.
use std::convert::TryInto;
use std::str::FromStr;

use lazy_static::lazy_static;
use regex::Regex;

use crate::next::document::error::{DocumentIdError, DocumentViewIdError};
use crate::next::document::{DocumentId, DocumentViewId};
use crate::next::operation::error::RelationListError;
use crate::next::operation::plain::{PlainFields, PlainValue};
use crate::next::operation::{
    OperationFields, OperationValue, PinnedRelation, PinnedRelationList, Relation, RelationList,
};
use crate::next::schema::error::{
    SchemaDefinitionError, SchemaFieldDefinitionError, ValidationError,
};
use crate::next::schema::{FieldName, FieldType, Schema, SchemaId};

/// Checks "name" field in a schema field definition operation.
///
/// 1. It must be at most 64 characters long
/// 2. It begins with a letter
/// 3. It uses only alphanumeric characters, digits and the underscore character ( _ )
fn check_schema_field_definition_name(value: &str) -> bool {
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
fn check_schema_field_definition_type(value: &str) -> bool {
    match value {
        "bool" | "int" | "float" | "str" => true,
        relation => check_schema_field_definition_relation(relation),
    }
}

/// Checks format for "type" fields which specify a relation.
///
/// 1. The first section is the name, which must have 1-64 characters, must start with a letter and
///    must contain only alphanumeric characters and underscores
/// 2. The remaining sections are the document view id, represented as alphabetically sorted
///    hex-encoded operation ids, separated by underscores
fn check_schema_field_definition_relation(value: &str) -> bool {
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

/// 1. The name of a schema MUST be at most 64 characters long
/// 2. It begins with a letter
/// 3. It uses only alphanumeric characters, digits and the underscore character ( _ )
/// 4. It doesn't end with an underscore
fn check_schema_definition_name(value: &str) -> bool {
    lazy_static! {
        // Unwrap as we checked the regular expression for correctness
        static ref NAME_REGEX: Regex = Regex::new(
            "^[A-Za-z]{1}[A-Za-z0-9_]{0,62}[A-Za-z0-9]{1}$
        ").unwrap();
    }

    NAME_REGEX.is_match(value)
}

/// 1. The description of a schema MUST consist of unicode characters
/// 2. ... and MUST be at most 256 characters long
fn check_schema_definition_description(value: &str) -> bool {
    value.chars().count() <= 256
}

/// A schema MUST have at most 1024 fields
fn check_schema_definition_fields(value: &Vec<Vec<String>>) -> bool {
    value.len() <= 1024
}

/// Checks if all fields of the schema match with the operation fields.
///
/// This can be used to safely validate a CREATE operation, as this operation needs to contain
/// _all_ fields of the schema.
///
/// The following validation steps are applied:
///
/// 1. Pinned relations (document view id), pinned relation lists and relation lists are sorted in
///    canonic format and without duplicates when no semantic value is given by that (#OP3)
/// 2. Operation fields match the claimed schema (#OP4)
///
/// Please note: This does NOT validate if the related document or view follows the given schema.
/// This can only be done with knowledge about external documents which requires a persistence
/// layer and is usually handled during materialization.
pub fn validate_all_fields(
    fields: &PlainFields,
    schema: &Schema,
) -> Result<OperationFields, ValidationError> {
    let mut validated_fields = OperationFields::new();
    let mut plain_fields = fields.iter();

    // Iterate through both field lists at the same time. Both `Schema` and `PlainFields` uses a
    // `BTreeMap` internally which gives us the guarantee that all fields are sorted. Through this
    // ordering we can compare them easily.
    for schema_field in schema.fields() {
        match plain_fields.next() {
            Some((plain_name, plain_value)) => {
                let (validated_name, validated_value) =
                    validate_field((plain_name, plain_value), schema_field).map_err(|err| {
                        ValidationError::InvalidField(plain_name.to_owned(), err.to_string())
                    })?;

                validated_fields
                    .insert(validated_name, validated_value)
                    // Unwrap here as we already checked during deserialization and population of
                    // the plain fields that there are no duplicates
                    .expect("Duplicate key name detected in plain fields");

                Ok(())
            }
            None => Err(ValidationError::MissingField(
                schema_field.0.to_owned(),
                schema_field.1.to_string(),
            )),
        }?;
    }

    // Collect last fields (if there is any) we can consider unexpected
    let unexpected_fields: Vec<FieldName> =
        plain_fields.map(|field| format!("'{}'", field.0)).collect();

    if unexpected_fields.is_empty() {
        Ok(validated_fields)
    } else {
        Err(ValidationError::UnexpectedFields(
            unexpected_fields.join(", "),
        ))
    }
}

/// Checks if given operation fields match the schema.
///
/// This can be used to safely validate an UPDATE operation, as this operation needs to contain at
/// least only _one_ fields of the schema.
///
/// The following validation steps are applied:
///
/// 1. Pinned relations (document view id), pinned relation lists and relation lists are sorted in
///    canonic format and without duplicates when no semantic value is given by that (#OP3)
/// 2. Operation fields match the claimed schema (#OP4)
///
/// Please note: This does NOT validate if the related document or view follows the given schema.
/// This can only be done with knowledge about external documents which requires a persistence
/// layer and is usually handled during materialization.
pub fn validate_only_given_fields(
    fields: &PlainFields,
    schema: &Schema,
) -> Result<OperationFields, ValidationError> {
    let mut validated_fields = OperationFields::new();
    let mut unexpected_fields: Vec<FieldName> = Vec::new();

    // Go through all given plain fields and check if they are known to the schema
    for (plain_name, plain_value) in fields.iter() {
        match schema.fields().get(plain_name) {
            Some(schema_field) => {
                let (validated_name, validated_value) =
                    validate_field((plain_name, plain_value), (plain_name, schema_field)).map_err(
                        |err| ValidationError::InvalidField(plain_name.to_owned(), err.to_string()),
                    )?;

                validated_fields
                    .insert(validated_name, validated_value)
                    // Unwrap here as we already checked during deserialization and population of
                    // the plain fields that there are no duplicates
                    .expect("Duplicate key name detected in plain fields");
            }
            None => {
                // Found a field which is not known to schema! We add it to a list so we can
                // display it later in an error message
                unexpected_fields.push(format!("'{}'", plain_name));
            }
        };
    }

    if unexpected_fields.is_empty() {
        Ok(validated_fields)
    } else {
        Err(ValidationError::UnexpectedFields(
            unexpected_fields.join(", "),
        ))
    }
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
        if check_schema_definition_name(value) {
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
            if check_schema_definition_description(value) {
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
            if check_schema_definition_fields(value) {
                Ok(())
            } else {
                Err(SchemaDefinitionError::FieldsInvalid)
            }
        }
        _ => Err(SchemaDefinitionError::FieldsWrongType),
    }?;

    Ok(())
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
            if check_schema_field_definition_name(value) {
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
            if check_schema_field_definition_type(value) {
                Ok(())
            } else {
                Err(SchemaFieldDefinitionError::TypeInvalid)
            }
        }
        _ => Err(SchemaFieldDefinitionError::TypeWrongType),
    }?;

    Ok(())
}

/// Validates name and type of an operation field by matching it against a schema field.
fn validate_field<'a>(
    plain_field: (&'a FieldName, &PlainValue),
    schema_field: (&FieldName, &FieldType),
) -> Result<(&'a FieldName, OperationValue), ValidationError> {
    let validated_name = validate_field_name(plain_field.0, schema_field.0)?;
    let validated_value = validate_field_value(plain_field.1, schema_field.1)?;
    Ok((validated_name, validated_value))
}

/// Validates name of an operation field by matching it against a schema field name.
fn validate_field_name<'a>(
    plain_field_name: &'a FieldName,
    schema_field_name: &FieldName,
) -> Result<&'a FieldName, ValidationError> {
    if plain_field_name == schema_field_name {
        Ok(plain_field_name)
    } else {
        Err(ValidationError::InvalidName(
            plain_field_name.to_owned(),
            schema_field_name.to_owned(),
        ))
    }
}

/// Validates value of an operation field by matching it against a schema field type.
fn validate_field_value(
    plain_value: &PlainValue,
    schema_field_type: &FieldType,
) -> Result<OperationValue, ValidationError> {
    match schema_field_type {
        FieldType::Boolean => {
            if let PlainValue::Boolean(bool) = plain_value {
                Ok(OperationValue::Boolean(*bool))
            } else {
                Err(ValidationError::InvalidType(
                    plain_value.field_type().to_owned(),
                    schema_field_type.to_string(),
                ))
            }
        }
        FieldType::Integer => {
            if let PlainValue::Integer(int) = plain_value {
                Ok(OperationValue::Integer(*int))
            } else {
                Err(ValidationError::InvalidType(
                    plain_value.field_type().to_owned(),
                    schema_field_type.to_string(),
                ))
            }
        }
        FieldType::Float => {
            if let PlainValue::Float(float) = plain_value {
                Ok(OperationValue::Float(*float))
            } else {
                Err(ValidationError::InvalidType(
                    plain_value.field_type().to_owned(),
                    schema_field_type.to_string(),
                ))
            }
        }
        FieldType::String => {
            if let PlainValue::StringOrRelation(str) = plain_value {
                Ok(OperationValue::String(str.to_owned()))
            } else {
                Err(ValidationError::InvalidType(
                    plain_value.field_type().to_owned(),
                    schema_field_type.to_string(),
                ))
            }
        }
        FieldType::Relation(_) => {
            if let PlainValue::StringOrRelation(document_id_str) = plain_value {
                // Convert string to document id, check for correctness
                let document_id: DocumentId =
                    document_id_str.parse().map_err(|err: DocumentIdError| {
                        ValidationError::InvalidValue(err.to_string())
                    })?;

                Ok(OperationValue::Relation(Relation::new(document_id)))
            } else {
                Err(ValidationError::InvalidType(
                    plain_value.field_type().to_owned(),
                    schema_field_type.to_string(),
                ))
            }
        }
        FieldType::RelationList(_) => {
            if let PlainValue::PinnedRelationOrRelationList(document_ids_str) = plain_value {
                // Convert list of strings to list of document ids aka a relation list
                let relation_list: RelationList =
                    document_ids_str
                        .as_slice()
                        .try_into()
                        .map_err(|err: RelationListError| {
                            // Detected an invalid document id
                            ValidationError::InvalidValue(err.to_string())
                        })?;

                // Note that we do NOT check for duplicates and ordering here as this information
                // is semantic!
                Ok(OperationValue::RelationList(relation_list))
            } else {
                Err(ValidationError::InvalidType(
                    plain_value.field_type().to_owned(),
                    schema_field_type.to_string(),
                ))
            }
        }
        FieldType::PinnedRelation(_) => {
            if let PlainValue::PinnedRelationOrRelationList(operation_ids_str) = plain_value {
                // Convert list of strings to list of operation ids aka a document view id, this
                // checks if list of operation ids is sorted and without any duplicates
                let document_view_id: DocumentViewId = operation_ids_str
                    .as_slice()
                    .try_into()
                    .map_err(|err: DocumentViewIdError| {
                        ValidationError::InvalidDocumentViewId(err.to_string())
                    })?;

                Ok(OperationValue::PinnedRelation(PinnedRelation::new(
                    document_view_id,
                )))
            } else {
                Err(ValidationError::InvalidType(
                    plain_value.field_type().to_owned(),
                    schema_field_type.to_string(),
                ))
            }
        }
        FieldType::PinnedRelationList(_) => {
            if let PlainValue::PinnedRelationList(document_view_ids_vec) = plain_value {
                let document_view_ids: Result<Vec<DocumentViewId>, ValidationError> =
                    document_view_ids_vec
                        .iter()
                        .map(|operation_ids_str| {
                            // Convert list of strings to list of operation ids aka a document view
                            // id, this checks if list of operation ids is sorted and without any
                            // duplicates
                            let document_view_id: DocumentViewId = operation_ids_str
                                .as_slice()
                                .try_into()
                                .map_err(|err: DocumentViewIdError| {
                                    ValidationError::InvalidDocumentViewId(err.to_string())
                                })?;

                            Ok(document_view_id)
                        })
                        .collect();

                // Note that we do NOT check for duplicates and ordering of the document view ids
                // as this information is semantic
                Ok(OperationValue::PinnedRelationList(PinnedRelationList::new(
                    document_view_ids?,
                )))
            } else {
                Err(ValidationError::InvalidType(
                    plain_value.field_type().to_owned(),
                    schema_field_type.to_string(),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::next::document::DocumentViewId;
    use crate::next::operation::plain::{PlainFields, PlainValue};
    use crate::next::operation::{OperationFields, OperationValue};
    use crate::next::schema::{FieldType, Schema, SchemaId};
    use crate::next::test_utils::constants::{HASH, SCHEMA_ID};
    use crate::next::test_utils::fixtures::document_view_id;
    use crate::next::test_utils::fixtures::schema_id;

    use super::{
        validate_all_fields, validate_field, validate_field_name, validate_field_value,
        validate_only_given_fields,
    };

    #[test]
    fn correct_and_invalid_field() {
        // Field names and value types are matching
        assert!(validate_field(
            (
                &"cutest_animal_in_zoo".to_owned(),
                &PlainValue::StringOrRelation("Panda".into()),
            ),
            (&"cutest_animal_in_zoo".to_owned(), &FieldType::String)
        )
        .is_ok());

        // Wrong field name
        assert!(validate_field(
            (
                &"most_boring_animal_in_zoo".to_owned(),
                &PlainValue::StringOrRelation("Llama".into()),
            ),
            (&"cutest_animal_in_zoo".to_owned(), &FieldType::String)
        )
        .is_err());

        // Wrong field value
        assert!(validate_field(
            (
                &"most_boring_animal_in_zoo".to_owned(),
                &PlainValue::StringOrRelation("Llama".into()),
            ),
            (
                &"most_boring_animal_in_zoo".to_owned(),
                &FieldType::Relation(schema_id(SCHEMA_ID))
            )
        )
        .is_err());
    }

    #[test]
    fn field_name() {
        assert!(validate_field_name(&"same".to_owned(), &"same".to_owned()).is_ok());
        assert!(validate_field_name(&"but".to_owned(), &"different".to_owned()).is_err());
    }

    #[rstest]
    #[case(PlainValue::StringOrRelation("Handa".into()), FieldType::String)]
    #[case(PlainValue::Integer(512), FieldType::Integer)]
    #[case(PlainValue::Float(1024.32), FieldType::Float)]
    #[case(PlainValue::Boolean(true), FieldType::Boolean)]
    #[case(
        PlainValue::StringOrRelation(HASH.to_owned()),
        FieldType::Relation(schema_id(SCHEMA_ID))
    )]
    #[case(
        PlainValue::PinnedRelationOrRelationList(vec![HASH.to_owned()]),
        FieldType::PinnedRelation(schema_id(SCHEMA_ID))
    )]
    #[case(
        PlainValue::PinnedRelationOrRelationList(vec![HASH.to_owned()]),
        FieldType::RelationList(schema_id(SCHEMA_ID))
    )]
    #[case(
        PlainValue::PinnedRelationList(vec![vec![HASH.to_owned()]]),
        FieldType::PinnedRelationList(schema_id(SCHEMA_ID))
    )]
    fn correct_field_values(#[case] plain_value: PlainValue, #[case] schema_field_type: FieldType) {
        assert!(validate_field_value(&plain_value, &schema_field_type).is_ok());
    }

    #[rstest]
    #[case(
        PlainValue::StringOrRelation("The Zookeeper".into()),
        FieldType::Integer,
        "invalid field type 'str', expected 'int'",
    )]
    #[case(
        PlainValue::Integer(13),
        FieldType::String,
        "invalid field type 'int', expected 'str'"
    )]
    #[case(
        PlainValue::Boolean(true),
        FieldType::Float,
        "invalid field type 'bool', expected 'float'"
    )]
    #[case(
        PlainValue::Float(123.123),
        FieldType::Integer,
        "invalid field type 'float', expected 'int'"
    )]
    #[case(
        PlainValue::StringOrRelation(HASH.to_owned()),
        FieldType::RelationList(schema_id(SCHEMA_ID)),
        "invalid field type 'str', expected 'relation_list(venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b)'",
    )]
    fn wrong_field_values(
        #[case] plain_value: PlainValue,
        #[case] schema_field_type: FieldType,
        #[case] expected: &str,
    ) {
        assert_eq!(
            validate_field_value(&plain_value, &schema_field_type)
                .err()
                .expect("Expected error")
                .to_string(),
            expected.to_string()
        );
    }

    #[rstest]
    #[case(
        vec![
            ("message", FieldType::String),
            ("age", FieldType::Integer),
            ("fans", FieldType::RelationList(schema_id(SCHEMA_ID))),
        ],
        vec![
            ("message", PlainValue::StringOrRelation("Hello, Mr. Handa!".into())),
            ("age", PlainValue::Integer(41)),
            ("fans", PlainValue::PinnedRelationOrRelationList(vec![HASH.to_owned()])),
        ],
    )]
    #[case(
        vec![
            ("b", FieldType::String),
            ("a", FieldType::Integer),
            ("c", FieldType::Boolean),
        ],
        vec![
            ("c", PlainValue::Boolean(false)),
            ("b", PlainValue::StringOrRelation("Panda-San!".into())),
            ("a", PlainValue::Integer(6)),
        ],
    )]
    fn correct_all_fields(
        #[from(document_view_id)] schema_view_id: DocumentViewId,
        #[case] schema_fields: Vec<(&str, FieldType)>,
        #[case] fields: Vec<(&str, PlainValue)>,
    ) {
        // Construct a schema
        let schema = Schema::new(
            &SchemaId::Application("zoo".to_owned(), schema_view_id),
            "Some schema description",
            schema_fields,
        )
        .unwrap();

        // Construct plain fields
        let mut plain_fields = PlainFields::new();
        for (plain_field_name, plain_field_value) in fields {
            plain_fields
                .insert(plain_field_name, plain_field_value)
                .unwrap();
        }

        // Check if fields match the schema
        assert!(validate_all_fields(&plain_fields, &schema).is_ok());
    }

    #[rstest]
    // Unknown plain field
    #[case(
        vec![
            ("message", FieldType::String),
        ],
        vec![
            ("fans", PlainValue::PinnedRelationOrRelationList(vec![HASH.to_owned()])),
            ("message", PlainValue::StringOrRelation("Hello, Mr. Handa!".into())),
        ],
        "field 'fans' does not match schema: expected field name 'message'"
    )]
    // Missing plain field
    #[case(
        vec![
            ("age", FieldType::Integer),
            ("message", FieldType::String),
        ],
        vec![
            ("message", PlainValue::StringOrRelation("Panda-San!".into())),
        ],
        "field 'message' does not match schema: expected field name 'age'"
    )]
    // Wrong field type
    #[case(
        vec![
            ("is_boring", FieldType::Boolean),
            ("cuteness_level", FieldType::Float),
            ("name", FieldType::String),
        ],
        vec![
            ("is_boring", PlainValue::Boolean(false)),
            ("cuteness_level", PlainValue::StringOrRelation("Very high! I promise!".into())),
            ("name", PlainValue::StringOrRelation("The really not boring Llama!!!".into())),
        ],
        "field 'cuteness_level' does not match schema: invalid field type 'str', expected 'float'"
    )]
    // Wrong field name
    #[case(
        vec![
            ("is_boring", FieldType::Boolean),
        ],
        vec![
            ("is_cute", PlainValue::Boolean(false)),
        ],
        "field 'is_cute' does not match schema: expected field name 'is_boring'",
    )]
    fn wrong_all_fields(
        #[from(document_view_id)] schema_view_id: DocumentViewId,
        #[case] schema_fields: Vec<(&str, FieldType)>,
        #[case] fields: Vec<(&str, PlainValue)>,
        #[case] expected: &str,
    ) {
        // Construct a schema
        let schema = Schema::new(
            &SchemaId::Application("zoo".to_owned(), schema_view_id),
            "Some schema description",
            schema_fields,
        )
        .unwrap();

        // Construct plain fields
        let mut plain_fields = PlainFields::new();
        for (plain_field_name, plain_field_value) in fields {
            plain_fields
                .insert(plain_field_name, plain_field_value)
                .unwrap();
        }

        // Check if fields match the schema
        assert_eq!(
            validate_all_fields(&plain_fields, &schema)
                .err()
                .expect("Expected error")
                .to_string(),
            expected
        );
    }

    #[rstest]
    #[case(
        vec![
            ("message", FieldType::String),
            ("age", FieldType::Integer),
            ("is_cute", FieldType::Boolean),
        ],
        vec![
            ("message", PlainValue::StringOrRelation("Hello, Mr. Handa!".into())),
        ],
    )]
    #[case(
        vec![
            ("message", FieldType::String),
            ("age", FieldType::Integer),
            ("is_cute", FieldType::Boolean),
        ],
        vec![
            ("age", PlainValue::Integer(41)),
            ("message", PlainValue::StringOrRelation("Hello, Mr. Handa!".into())),
        ],
    )]
    fn correct_only_given_fields(
        #[from(document_view_id)] schema_view_id: DocumentViewId,
        #[case] schema_fields: Vec<(&str, FieldType)>,
        #[case] fields: Vec<(&str, PlainValue)>,
    ) {
        // Construct a schema
        let schema = Schema::new(
            &SchemaId::Application("zoo".to_owned(), schema_view_id),
            "Some schema description",
            schema_fields,
        )
        .unwrap();

        // Construct plain fields
        let mut plain_fields = PlainFields::new();
        for (plain_field_name, plain_field_value) in fields {
            plain_fields
                .insert(plain_field_name, plain_field_value)
                .unwrap();
        }

        // Check if fields match the schema
        assert!(validate_only_given_fields(&plain_fields, &schema).is_ok());
    }

    #[rstest]
    // Missing plain field
    #[case(
        vec![
            ("message", FieldType::String),
            ("age", FieldType::Integer),
            ("is_cute", FieldType::Boolean),
        ],
        vec![
            ("spam", PlainValue::StringOrRelation("PANDA IS THE CUTEST!".into())),
        ],
        "unexpected fields found: 'spam'",
    )]
    // Too many fields
    #[case(
        vec![
            ("age", FieldType::Integer),
            ("is_cute", FieldType::Boolean),
        ],
        vec![
            ("is_cute", PlainValue::Boolean(false)),
            ("age", PlainValue::Integer(41)),
            ("message", PlainValue::StringOrRelation("Hello, Mr. Handa!".into())),
            ("response", PlainValue::StringOrRelation("Good bye!".into())),
        ],
        "unexpected fields found: 'message', 'response'",
    )]
    // Wrong type
    #[case(
        vec![
            ("age", FieldType::Integer),
            ("is_cute", FieldType::Boolean),
        ],
        vec![
            ("age", PlainValue::Float(41.34)),
        ],
        "field 'age' does not match schema: invalid field type 'float', expected 'int'",
    )]
    // Wrong name
    #[case(
        vec![
            ("age", FieldType::Integer),
        ],
        vec![
            ("rage", PlainValue::Integer(100)),
        ],
        "unexpected fields found: 'rage'",
    )]
    fn wrong_only_given_fields(
        #[from(document_view_id)] schema_view_id: DocumentViewId,
        #[case] schema_fields: Vec<(&str, FieldType)>,
        #[case] fields: Vec<(&str, PlainValue)>,
        #[case] expected: &str,
    ) {
        // Construct a schema
        let schema = Schema::new(
            &SchemaId::Application("zoo".to_owned(), schema_view_id),
            "Some schema description",
            schema_fields,
        )
        .unwrap();

        // Construct plain fields
        let mut plain_fields = PlainFields::new();
        for (plain_field_name, plain_field_value) in fields {
            plain_fields
                .insert(&plain_field_name, plain_field_value)
                .unwrap();
        }

        // Check if fields match the schema
        assert_eq!(
            validate_only_given_fields(&plain_fields, &schema)
                .err()
                .expect("Expect error")
                .to_string(),
            expected
        );
    }

    #[rstest]
    fn conversion_to_operation_fields(#[from(document_view_id)] schema_view_id: DocumentViewId) {
        // Construct a schema
        let schema = Schema::new(
            &SchemaId::Application("polar".to_owned(), schema_view_id),
            "Some schema description",
            vec![
                ("icecream", FieldType::String),
                ("degree", FieldType::Float),
            ],
        )
        .unwrap();

        // Construct plain fields
        let mut plain_fields = PlainFields::new();
        plain_fields
            .insert("icecream", PlainValue::StringOrRelation("Almond".into()))
            .unwrap();
        plain_fields
            .insert("degree", PlainValue::Float(6.12))
            .unwrap();

        // Construct expected operation fields
        let mut fields = OperationFields::new();
        fields
            .insert("icecream", OperationValue::String("Almond".into()))
            .unwrap();
        fields
            .insert("degree", OperationValue::Float(6.12))
            .unwrap();

        // Verification methods should give us the validated operation fields
        assert_eq!(validate_all_fields(&plain_fields, &schema).unwrap(), fields);
        assert_eq!(
            validate_only_given_fields(&plain_fields, &schema).unwrap(),
            fields
        );
    }
}
