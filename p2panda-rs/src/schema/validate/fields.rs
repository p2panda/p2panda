// SPDX-License-Identifier: AGPL-&3.0-or-later

//! Various methods to validate an operation against a schema.
use std::convert::TryInto;

use crate::document::error::{DocumentIdError, DocumentViewIdError};
use crate::document::{DocumentId, DocumentViewId};
use crate::operation::error::RelationListError;
use crate::operation::plain::{PlainFields, PlainValue};
use crate::operation::{
    OperationFields, OperationValue, PinnedRelation, PinnedRelationList, Relation, RelationList,
};
use crate::schema::validate::error::ValidationError;
use crate::schema::validate::{
    validate_schema_definition_v1_fields, validate_schema_field_definition_v1_fields,
};
use crate::schema::{FieldName, FieldType, Schema, SchemaId};
use crate::serde::deserialize_into;

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
    for schema_field in schema.fields().iter() {
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

    // When given, check against special validation rules for system schemas
    validate_system_schema_fields(fields, schema)?;

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

    // When given, check against special validation rules for system schemas
    validate_system_schema_fields(fields, schema)?;

    if unexpected_fields.is_empty() {
        Ok(validated_fields)
    } else {
        Err(ValidationError::UnexpectedFields(
            unexpected_fields.join(", "),
        ))
    }
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
        FieldType::Bytes => {
            if let PlainValue::Bytes(bytes) = plain_value {
                Ok(OperationValue::Bytes(bytes.to_vec()))
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
            if let PlainValue::Bytes(_) = plain_value {
                let string_value = plain_value.try_into_string_from_utf8_bytes()?;
                Ok(OperationValue::String(string_value.to_owned()))
            } else {
                Err(ValidationError::InvalidType(
                    plain_value.field_type().to_owned(),
                    schema_field_type.to_string(),
                ))
            }
        }
        FieldType::Relation(_) => {
            if let PlainValue::Bytes(_) = plain_value {
                // Convert byte string to document id, check for correctness
                let string_value = plain_value.try_into_string_from_utf8_bytes()?;
                let document_id: DocumentId =
                    string_value.parse().map_err(|err: DocumentIdError| {
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
            match plain_value {
                PlainValue::AmbiguousRelation(document_ids_str) => {
                    // Convert list of strings to list of document ids aka a relation list
                    let relation_list: RelationList = document_ids_str
                        .as_slice()
                        .try_into()
                        .map_err(|err: RelationListError| {
                            // Detected an invalid document id
                            ValidationError::InvalidValue(err.to_string())
                        })?;

                    // Note that we do NOT check for duplicates and ordering here as this information
                    // is semantic!
                    Ok(OperationValue::RelationList(relation_list))
                }
                PlainValue::Bytes(byte_string) => {
                    // The only case where a byte_string is expected is when this value represents
                    // an empty relation list, so we validate here that this is indeed an empty
                    // vec of bytes.

                    if !byte_string.is_empty() {
                        Err(ValidationError::InvalidType(
                            plain_value.field_type().to_owned(),
                            schema_field_type.to_string(),
                        ))
                    } else {
                        Ok(OperationValue::RelationList(RelationList::new(vec![])))
                    }
                }
                _ => Err(ValidationError::InvalidType(
                    plain_value.field_type().to_owned(),
                    schema_field_type.to_string(),
                )),
            }
        }
        FieldType::PinnedRelation(_) => {
            if let PlainValue::AmbiguousRelation(operation_ids_str) = plain_value {
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
            match plain_value {
                PlainValue::PinnedRelationList(document_view_ids_vec) => {
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
                }
                PlainValue::Bytes(byte_string) => {
                    // The only case where a byte_string is expected is when this value represents
                    // an empty relation list, so we validate here that this is indeed an empty
                    // vec of bytes.

                    if !byte_string.is_empty() {
                        Err(ValidationError::InvalidType(
                            plain_value.field_type().to_owned(),
                            schema_field_type.to_string(),
                        ))
                    } else {
                        Ok(OperationValue::RelationList(RelationList::new(vec![])))
                    }
                }
                _ => Err(ValidationError::InvalidType(
                    plain_value.field_type().to_owned(),
                    schema_field_type.to_string(),
                )),
            }
        }
    }
}

/// Method to validate operation fields against special formatting rules of system schemas.
fn validate_system_schema_fields(
    fields: &PlainFields,
    schema: &Schema,
) -> Result<(), ValidationError> {
    match schema.id() {
        SchemaId::Application(_, _) => Ok(()),
        SchemaId::SchemaDefinition(_) => {
            validate_schema_definition_v1_fields(fields)?;
            Ok(())
        }
        SchemaId::SchemaFieldDefinition(_) => {
            validate_schema_field_definition_v1_fields(fields)?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_bytes::ByteBuf;

    use crate::document::DocumentViewId;
    use crate::operation::plain::{PlainFields, PlainValue};
    use crate::operation::{OperationFields, OperationValue};
    use crate::schema::{FieldType, Schema, SchemaId, SchemaName};
    use crate::test_utils::constants::{HASH, SCHEMA_ID};
    use crate::test_utils::fixtures::document_view_id;
    use crate::test_utils::fixtures::schema_id;

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
                &PlainValue::Bytes(ByteBuf::from("Panda")),
            ),
            (&"cutest_animal_in_zoo".to_owned(), &FieldType::String)
        )
        .is_ok());

        // Wrong field name
        assert!(validate_field(
            (
                &"most_boring_animal_in_zoo".to_owned(),
                &PlainValue::Bytes(ByteBuf::from("Llama")),
            ),
            (&"cutest_animal_in_zoo".to_owned(), &FieldType::String)
        )
        .is_err());

        // Wrong field value
        assert!(validate_field(
            (
                &"most_boring_animal_in_zoo".to_owned(),
                &PlainValue::Bytes(ByteBuf::from("Llama")),
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
    #[case(PlainValue::Bytes(ByteBuf::from("Handa")), FieldType::String)]
    #[case(PlainValue::Integer(512), FieldType::Integer)]
    #[case(PlainValue::Float(1024.32), FieldType::Float)]
    #[case(PlainValue::Boolean(true), FieldType::Boolean)]
    #[case(
        PlainValue::Bytes(ByteBuf::from(HASH)),
        FieldType::Relation(schema_id(SCHEMA_ID))
    )]
    #[case(
        PlainValue::AmbiguousRelation(vec![HASH.to_owned()]),
        FieldType::PinnedRelation(schema_id(SCHEMA_ID))
    )]
    #[case(
        PlainValue::AmbiguousRelation(vec![HASH.to_owned()]),
        FieldType::RelationList(schema_id(SCHEMA_ID))
    )]
    #[case(
        PlainValue::AmbiguousRelation(vec![]),
        FieldType::RelationList(schema_id(SCHEMA_ID))
    )]
    #[case(
        PlainValue::PinnedRelationList(vec![vec![HASH.to_owned()]]),
        FieldType::PinnedRelationList(schema_id(SCHEMA_ID))
    )]
    #[case(
        PlainValue::PinnedRelationList(vec![]),
        FieldType::PinnedRelationList(schema_id(SCHEMA_ID))
    )]
    fn correct_field_values(#[case] plain_value: PlainValue, #[case] schema_field_type: FieldType) {
        let result = validate_field_value(&plain_value, &schema_field_type);
        assert!(result.is_ok(), "{:#?}", result);
    }

    #[rstest]
    #[case(
        PlainValue::Bytes(ByteBuf::from("The Zookeeper")),
        FieldType::Integer,
        "invalid field type 'byte_string', expected 'int'"
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
        PlainValue::Bytes(ByteBuf::from(HASH)),
        FieldType::RelationList(schema_id(SCHEMA_ID)),
        "invalid field type 'byte_string', expected 'relation_list(venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b)'",
    )]
    fn wrong_field_values(
        #[case] plain_value: PlainValue,
        #[case] schema_field_type: FieldType,
        #[case] expected: &str,
    ) {
        assert_eq!(
            validate_field_value(&plain_value, &schema_field_type)
                .expect_err("Expected error")
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
            ("message", PlainValue::Bytes(ByteBuf::from("Hello, Mr. Handa!"))),
            ("age", PlainValue::Integer(41)),
            ("fans", PlainValue::AmbiguousRelation(vec![HASH.to_owned()])),
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
            ("b", PlainValue::Bytes(ByteBuf::from("Panda-San!"))),
            ("a", PlainValue::Integer(6)),
        ],
    )]
    #[case(
        vec![
            ("a", FieldType::PinnedRelationList(schema_id(SCHEMA_ID))),
        ],
        vec![
            ("a", PlainValue::PinnedRelationList(vec![])),
        ],
    )]
    #[case(
        vec![
            ("a", FieldType::PinnedRelationList(schema_id(SCHEMA_ID))),
        ],
        vec![
            ("a", PlainValue::Bytes(ByteBuf::from([]))),
        ],
    )]
    fn correct_all_fields(
        #[from(document_view_id)] schema_view_id: DocumentViewId,
        #[case] schema_fields: Vec<(&str, FieldType)>,
        #[case] fields: Vec<(&str, PlainValue)>,
    ) {
        // Construct a schema
        let schema_name = SchemaName::new("zoo").expect("Valid schema name");
        let schema = Schema::new(
            &SchemaId::Application(schema_name, schema_view_id),
            "Some schema description",
            &schema_fields,
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
            ("fans", PlainValue::AmbiguousRelation(vec![HASH.to_owned()])),
            ("message", PlainValue::Bytes(ByteBuf::from("Hello, Mr. Handa!"))),
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
            ("message", PlainValue::Bytes(ByteBuf::from("Panda-San!"))),
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
            ("cuteness_level", PlainValue::Bytes(ByteBuf::from("Very high! I promise!"))),
            ("name", PlainValue::Bytes(ByteBuf::from("The really not boring Llama!!!"))),
        ],
        "field 'cuteness_level' does not match schema: invalid field type 'byte_string', expected 'float'"
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
        let schema_name = SchemaName::new("zoo").expect("Valid schema name");
        let schema = Schema::new(
            &SchemaId::Application(schema_name.to_owned(), schema_view_id),
            "Some schema description",
            &schema_fields,
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
                .expect_err("Expected error")
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
            ("message", PlainValue::Bytes(ByteBuf::from("Hello, Mr. Handa!"))),
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
            ("message", PlainValue::Bytes(ByteBuf::from("Hello, Mr. Handa!"))),
        ],
    )]
    fn correct_only_given_fields(
        #[from(document_view_id)] schema_view_id: DocumentViewId,
        #[case] schema_fields: Vec<(&str, FieldType)>,
        #[case] fields: Vec<(&str, PlainValue)>,
    ) {
        // Construct a schema
        let schema_name = SchemaName::new("zoo").expect("Valid schema name");
        let schema = Schema::new(
            &SchemaId::Application(schema_name, schema_view_id),
            "Some schema description",
            &schema_fields,
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
            ("spam", PlainValue::Bytes(ByteBuf::from("PANDA IS THE CUTEST!"))),
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
            ("message", PlainValue::Bytes(ByteBuf::from("Hello, Mr. Handa!"))),
            ("response", PlainValue::Bytes(ByteBuf::from("Good bye!"))),
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
        let schema_name = SchemaName::new("zoo").expect("Valid schema name");
        let schema = Schema::new(
            &SchemaId::Application(schema_name, schema_view_id),
            "Some schema description",
            &schema_fields,
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
            validate_only_given_fields(&plain_fields, &schema)
                .expect_err("Expect error")
                .to_string(),
            expected
        );
    }

    #[rstest]
    fn conversion_to_operation_fields(#[from(document_view_id)] schema_view_id: DocumentViewId) {
        // Construct a schema
        let schema_name = SchemaName::new("polar").expect("Valid schema name");
        let schema = Schema::new(
            &SchemaId::Application(schema_name, schema_view_id),
            "Some schema description",
            &[
                ("icecream", FieldType::String),
                ("degree", FieldType::Float),
            ],
        )
        .unwrap();

        // Construct plain fields
        let mut plain_fields = PlainFields::new();
        plain_fields
            .insert("icecream", PlainValue::Bytes(ByteBuf::from("Almond")))
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

    #[rstest]
    #[case::unknown_fields(
        SchemaId::SchemaDefinition(1),
        vec![
            ("fans", PlainValue::AmbiguousRelation(vec![HASH.to_owned()])),
        ],
        "field 'fans' does not match schema: expected field name 'description'"
    )]
    #[case::invalid_type(
        SchemaId::SchemaDefinition(1),
        vec![
            ("name", "venue".into()),
            ("description", "A short description".into()),
            ("fields", "This is not a pinned relation list".into()),
        ],
        "field 'fields' does not match schema: invalid field type 'byte_string', expected 'pinned_relation_list(schema_field_definition_v1)'"
    )]
    #[case::invalid_name(
        SchemaId::SchemaDefinition(1),
        vec![
            ("name", "__invalid_name__".into()),
            ("description", "A short description".into()),
            ("fields", PlainValue::PinnedRelationList(vec![vec![HASH.to_owned()]]))
        ],
        "invalid 'schema_definition_v1' operation: 'name' field in schema field definitions is wrongly formatted"
    )]
    #[case::invalid_field_type(
        SchemaId::SchemaFieldDefinition(1),
        vec![
            ("name", "is_cute".into()),
            ("type", "floatyboaty".into()),
        ],
        "invalid 'schema_field_definition_v1' operation: 'type' field in schema field definitions is wrongly formatted"
    )]
    fn wrong_system_schema_operations(
        #[case] schema_id: SchemaId,
        #[case] fields: Vec<(&str, PlainValue)>,
        #[case] expected: &str,
    ) {
        // Get system schema struct
        let schema = Schema::get_system(schema_id).unwrap();

        // Construct plain fields
        let mut plain_fields = PlainFields::new();
        for (plain_field_name, plain_field_value) in fields {
            plain_fields
                .insert(plain_field_name, plain_field_value)
                .unwrap();
        }

        // Check if fields match the schema
        assert_eq!(
            validate_all_fields(&plain_fields, schema)
                .expect_err("Expected error")
                .to_string(),
            expected
        );
    }

    #[rstest]
    #[case(
        SchemaId::SchemaDefinition(1),
        vec![
            ("name", "venue".into()),
            ("description", "A short description".into()),
            ("fields", PlainValue::PinnedRelationList(vec![vec![HASH.to_owned()]]))
        ],
    )]
    #[case(
        SchemaId::SchemaFieldDefinition(1),
        vec![
            ("name", "is_cute".into()),
            ("type", "bool".into()),
        ],
    )]
    fn correct_system_schema_operations(
        #[case] schema_id: SchemaId,
        #[case] fields: Vec<(&str, PlainValue)>,
    ) {
        // Get system schema struct
        let schema = Schema::get_system(schema_id).unwrap();

        // Construct plain fields
        let mut plain_fields = PlainFields::new();
        for (plain_field_name, plain_field_value) in fields {
            plain_fields
                .insert(plain_field_name, plain_field_value)
                .unwrap();
        }

        // Check if fields match the schema
        assert!(validate_all_fields(&plain_fields, schema).is_ok());
    }
}
