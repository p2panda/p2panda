// SPDX-License-Identifier: AGPL-&3.0-or-later

use crate::operation::{
    OperationFields, OperationValue, PinnedRelation, PinnedRelationList, RawFields, RawValue,
    Relation, RelationList,
};
use crate::schema::{FieldName, FieldType, Schema, ValidationError};

/// @TODO
///
/// Both `Schema` and `RawFields` uses a `BTreeMap` internally which gives us the guarantee that
/// all fields are sorted. Through this ordering we can compare them easily.
pub fn verify_all_fields(
    fields: &RawFields,
    schema: &Schema,
) -> Result<OperationFields, ValidationError> {
    let mut validated_fields = OperationFields::new();
    let mut raw_fields = fields.iter();

    // Iterate through both field lists at the same time, since they are both sorted already we can
    // compare them in every iteration step
    for schema_field in schema.fields() {
        match raw_fields.next() {
            Some((raw_name, raw_value)) => {
                let (validated_name, validated_value) =
                    verify_field((raw_name, raw_value), schema_field).map_err(|err| {
                        ValidationError::InvalidField(raw_name.to_owned(), err.to_string())
                    })?;

                validated_fields
                    .insert(&validated_name, validated_value)
                    // Unwrap here as we already checked during deserialization and population of
                    // the raw fields that there are no duplicates
                    .expect("Duplicate key name detected in raw fields");

                Ok(())
            }
            None => Err(ValidationError::MissingField(
                schema_field.0.to_owned(),
                schema_field.1.serialise(),
            )),
        }?;
    }

    // Collect last fields (if there is any) we can consider unexpected
    let unexpected_fields: Vec<FieldName> =
        raw_fields.map(|field| format!("'{}'", field.0)).collect();

    if unexpected_fields.is_empty() {
        Ok(validated_fields)
    } else {
        Err(ValidationError::UnexpectedFields(
            unexpected_fields.join(", "),
        ))
    }
}

pub fn verify_only_given_fields(
    fields: &RawFields,
    schema: &Schema,
) -> Result<OperationFields, ValidationError> {
    let mut validated_fields = OperationFields::new();
    let mut unexpected_fields: Vec<FieldName> = Vec::new();

    // Go through all given raw fields and check if they are known to the schema
    for (raw_name, raw_value) in fields.iter() {
        match schema.fields().get(raw_name) {
            Some(schema_field) => {
                let (validated_name, validated_value) =
                    verify_field((raw_name, raw_value), (raw_name, schema_field)).map_err(
                        |err| ValidationError::InvalidField(raw_name.to_owned(), err.to_string()),
                    )?;

                validated_fields
                    .insert(&validated_name, validated_value)
                    // Unwrap here as we already checked during deserialization and population of
                    // the raw fields that there are no duplicates
                    .expect("Duplicate key name detected in raw fields");
            }
            None => {
                // Found a field which is not known to schema! We add it to a list so we can
                // display it later in an error message
                unexpected_fields.push(format!("'{}'", raw_name.to_owned()));
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

fn verify_field<'a>(
    raw_field: (&'a FieldName, &RawValue),
    schema_field: (&FieldName, &FieldType),
) -> Result<(&'a FieldName, OperationValue), ValidationError> {
    let validated_name = verify_field_name(raw_field.0, schema_field.0)?;
    let validated_value = verify_field_value(raw_field.1, schema_field.1)?;
    Ok((validated_name, validated_value))
}

fn verify_field_name<'a>(
    raw_field_name: &'a FieldName,
    schema_field_name: &FieldName,
) -> Result<&'a FieldName, ValidationError> {
    if raw_field_name == schema_field_name {
        Ok(raw_field_name)
    } else {
        Err(ValidationError::InvalidName(
            raw_field_name.to_owned(),
            schema_field_name.to_owned(),
        ))
    }
}

/// Note: This does NOT verify if the pinned document view follows the given schema
fn verify_field_value(
    raw_value: &RawValue,
    schema_field_type: &FieldType,
) -> Result<OperationValue, ValidationError> {
    match schema_field_type {
        FieldType::Boolean => {
            if let RawValue::Boolean(bool) = raw_value {
                Ok(OperationValue::Boolean(*bool))
            } else {
                Err(ValidationError::InvalidType(
                    raw_value.field_type().to_owned(),
                    schema_field_type.serialise(),
                ))
            }
        }
        FieldType::Integer => {
            if let RawValue::Integer(int) = raw_value {
                Ok(OperationValue::Integer(*int))
            } else {
                Err(ValidationError::InvalidType(
                    raw_value.field_type().to_owned(),
                    schema_field_type.serialise(),
                ))
            }
        }
        FieldType::Float => {
            if let RawValue::Float(float) = raw_value {
                Ok(OperationValue::Float(*float))
            } else {
                Err(ValidationError::InvalidType(
                    raw_value.field_type().to_owned(),
                    schema_field_type.serialise(),
                ))
            }
        }
        FieldType::Text => {
            if let RawValue::Text(str) = raw_value {
                Ok(OperationValue::Text(str.to_owned()))
            } else {
                Err(ValidationError::InvalidType(
                    raw_value.field_type().to_owned(),
                    schema_field_type.serialise(),
                ))
            }
        }
        FieldType::Relation(_) => {
            if let RawValue::Relation(document_id) = raw_value {
                Ok(OperationValue::Relation(Relation::new(
                    document_id.to_owned(),
                )))
            } else {
                Err(ValidationError::InvalidType(
                    raw_value.field_type().to_owned(),
                    schema_field_type.serialise(),
                ))
            }
        }
        FieldType::RelationList(_) => {
            if let RawValue::RelationList(document_ids) = raw_value {
                // @TODO: Is this sorted? Are there duplicates?
                Ok(OperationValue::RelationList(RelationList::new(
                    document_ids.to_owned(),
                )))
            } else {
                Err(ValidationError::InvalidType(
                    raw_value.field_type().to_owned(),
                    schema_field_type.serialise(),
                ))
            }
        }
        FieldType::PinnedRelation(_) => {
            if let RawValue::PinnedRelation(document_view_id) = raw_value {
                // @TODO: Is this sorted? Are there duplicates?
                Ok(OperationValue::PinnedRelation(PinnedRelation::new(
                    document_view_id.to_owned(),
                )))
            } else {
                Err(ValidationError::InvalidType(
                    raw_value.field_type().to_owned(),
                    schema_field_type.serialise(),
                ))
            }
        }
        FieldType::PinnedRelationList(_) => {
            if let RawValue::PinnedRelationList(document_view_ids) = raw_value {
                // @TODO: Is this sorted? Are there duplicates?
                Ok(OperationValue::PinnedRelationList(PinnedRelationList::new(
                    document_view_ids.to_owned(),
                )))
            } else {
                Err(ValidationError::InvalidType(
                    raw_value.field_type().to_owned(),
                    schema_field_type.serialise(),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::DocumentViewId;
    use crate::operation::{OperationFields, OperationValue, RawFields, RawValue};
    use crate::schema::{FieldType, Schema, SchemaId};
    use crate::test_utils::constants::{HASH, SCHEMA_ID};
    use crate::test_utils::fixtures::{document_id, document_view_id, schema};

    use super::{
        verify_all_fields, verify_field, verify_field_name, verify_field_value,
        verify_only_given_fields,
    };

    #[test]
    fn correct_and_invalid_field() {
        // Field names and value types are matching
        assert!(verify_field(
            (
                &"cutest_animal_in_zoo".to_owned(),
                &RawValue::Text("Panda".into()),
            ),
            (&"cutest_animal_in_zoo".to_owned(), &FieldType::Text)
        )
        .is_ok());

        // Wrong field name
        assert!(verify_field(
            (
                &"most_boring_animal_in_zoo".to_owned(),
                &RawValue::Text("Llama".into()),
            ),
            (&"cutest_animal_in_zoo".to_owned(), &FieldType::Text)
        )
        .is_err());

        // Wrong field value
        assert!(verify_field(
            (
                &"most_boring_animal_in_zoo".to_owned(),
                &RawValue::Text("Llama".into()),
            ),
            (
                &"most_boring_animal_in_zoo".to_owned(),
                &FieldType::Relation(schema(SCHEMA_ID))
            )
        )
        .is_err());
    }

    #[test]
    fn field_name() {
        assert!(verify_field_name(&"same".to_owned(), &"same".to_owned()).is_ok());
        assert!(verify_field_name(&"but".to_owned(), &"different".to_owned()).is_err());
    }

    #[rstest]
    #[case(RawValue::Text("Handa".into()), FieldType::Text)]
    #[case(RawValue::Integer(512), FieldType::Integer)]
    #[case(RawValue::Float(1024.32), FieldType::Float)]
    #[case(RawValue::Boolean(true), FieldType::Boolean)]
    #[case(
        RawValue::Relation(document_id(HASH)),
        FieldType::Relation(schema(SCHEMA_ID))
    )]
    #[case(
        RawValue::PinnedRelation(document_view_id(vec![HASH])),
        FieldType::PinnedRelation(schema(SCHEMA_ID))
    )]
    #[case(
        RawValue::RelationList(vec![document_id(HASH)]),
        FieldType::RelationList(schema(SCHEMA_ID))
    )]
    #[case(
        RawValue::PinnedRelationList(vec![document_view_id(vec![HASH])]),
        FieldType::PinnedRelationList(schema(SCHEMA_ID))
    )]
    fn correct_field_values(#[case] raw_value: RawValue, #[case] schema_field_type: FieldType) {
        assert!(verify_field_value(&raw_value, &schema_field_type).is_ok());
    }

    #[rstest]
    #[case(
        RawValue::Text("The Zookeeper".into()),
        FieldType::Integer,
        "invalid field type 'str', expected 'int'",
    )]
    #[case(
        RawValue::Integer(13),
        FieldType::Text,
        "invalid field type 'int', expected 'str'"
    )]
    #[case(
        RawValue::Boolean(true),
        FieldType::Float,
        "invalid field type 'bool', expected 'float'"
    )]
    #[case(
        RawValue::Float(123.123),
        FieldType::Integer,
        "invalid field type 'float', expected 'int'"
    )]
    #[case(
        RawValue::PinnedRelation(document_view_id(vec![HASH])),
        FieldType::RelationList(schema(SCHEMA_ID)),
        "invalid field type 'pinned_relation', expected 'relation_list(venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b)'",
    )]
    fn wrong_field_values(
        #[case] raw_value: RawValue,
        #[case] schema_field_type: FieldType,
        #[case] expected: &str,
    ) {
        assert_eq!(
            verify_field_value(&raw_value, &schema_field_type)
                .err()
                .expect("Expected error")
                .to_string(),
            expected.to_string()
        );
    }

    #[rstest]
    #[case(
        vec![
            ("message", FieldType::Text),
            ("age", FieldType::Integer),
            ("fans", FieldType::RelationList(schema(SCHEMA_ID))),
        ],
        vec![
            ("message", RawValue::Text("Hello, Mr. Handa!".into())),
            ("age", RawValue::Integer(41)),
            ("fans", RawValue::RelationList(vec![document_id(HASH)])),
        ],
    )]
    #[case(
        vec![
            ("b", FieldType::Text),
            ("a", FieldType::Integer),
            ("c", FieldType::Boolean),
        ],
        vec![
            ("c", RawValue::Boolean(false)),
            ("b", RawValue::Text("Panda-San!".into())),
            ("a", RawValue::Integer(6)),
        ],
    )]
    fn correct_all_fields(
        #[from(document_view_id)] schema_view_id: DocumentViewId,
        #[case] schema_fields: Vec<(&str, FieldType)>,
        #[case] fields: Vec<(&str, RawValue)>,
    ) {
        // Construct a schema
        let schema = Schema::new(
            &SchemaId::Application("zoo".to_owned(), schema_view_id),
            "Some schema description",
            schema_fields,
        )
        .unwrap();

        // Construct raw fields
        let mut raw_fields = RawFields::new();
        for (raw_field_name, raw_field_value) in fields {
            raw_fields.insert(&raw_field_name, raw_field_value).unwrap();
        }

        // Check if fields match the schema
        assert!(verify_all_fields(&raw_fields, &schema).is_ok());
    }

    #[rstest]
    // Unknown raw field
    #[case(
        vec![
            ("message", FieldType::Text),
        ],
        vec![
            ("fans", RawValue::RelationList(vec![document_id(HASH)])),
            ("message", RawValue::Text("Hello, Mr. Handa!".into())),
        ],
        "field 'fans' does not match schema: expected field name 'message'"
    )]
    // Missing raw field
    #[case(
        vec![
            ("age", FieldType::Integer),
            ("message", FieldType::Text),
        ],
        vec![
            ("message", RawValue::Text("Panda-San!".into())),
        ],
        "field 'message' does not match schema: expected field name 'age'"
    )]
    // Wrong field type
    #[case(
        vec![
            ("is_boring", FieldType::Boolean),
            ("cuteness_level", FieldType::Float),
            ("name", FieldType::Text),
        ],
        vec![
            ("is_boring", RawValue::Boolean(false)),
            ("cuteness_level", RawValue::Text("Very high! I promise!".into())),
            ("name", RawValue::Text("The really not boring Llama!!!".into())),
        ],
        "field 'cuteness_level' does not match schema: invalid field type 'str', expected 'float'"
    )]
    // Wrong field name
    #[case(
        vec![
            ("is_boring", FieldType::Boolean),
        ],
        vec![
            ("is_cute", RawValue::Boolean(false)),
        ],
        "field 'is_cute' does not match schema: expected field name 'is_boring'",
    )]
    fn wrong_all_fields(
        #[from(document_view_id)] schema_view_id: DocumentViewId,
        #[case] schema_fields: Vec<(&str, FieldType)>,
        #[case] fields: Vec<(&str, RawValue)>,
        #[case] expected: &str,
    ) {
        // Construct a schema
        let schema = Schema::new(
            &SchemaId::Application("zoo".to_owned(), schema_view_id),
            "Some schema description",
            schema_fields,
        )
        .unwrap();

        // Construct raw fields
        let mut raw_fields = RawFields::new();
        for (raw_field_name, raw_field_value) in fields {
            raw_fields.insert(&raw_field_name, raw_field_value).unwrap();
        }

        // Check if fields match the schema
        assert_eq!(
            verify_all_fields(&raw_fields, &schema)
                .err()
                .expect("Expected error")
                .to_string(),
            expected
        );
    }

    #[rstest]
    #[case(
        vec![
            ("message", FieldType::Text),
            ("age", FieldType::Integer),
            ("is_cute", FieldType::Boolean),
        ],
        vec![
            ("message", RawValue::Text("Hello, Mr. Handa!".into())),
        ],
    )]
    #[case(
        vec![
            ("message", FieldType::Text),
            ("age", FieldType::Integer),
            ("is_cute", FieldType::Boolean),
        ],
        vec![
            ("age", RawValue::Integer(41)),
            ("message", RawValue::Text("Hello, Mr. Handa!".into())),
        ],
    )]
    fn correct_only_given_fields(
        #[from(document_view_id)] schema_view_id: DocumentViewId,
        #[case] schema_fields: Vec<(&str, FieldType)>,
        #[case] fields: Vec<(&str, RawValue)>,
    ) {
        // Construct a schema
        let schema = Schema::new(
            &SchemaId::Application("zoo".to_owned(), schema_view_id),
            "Some schema description",
            schema_fields,
        )
        .unwrap();

        // Construct raw fields
        let mut raw_fields = RawFields::new();
        for (raw_field_name, raw_field_value) in fields {
            raw_fields.insert(&raw_field_name, raw_field_value).unwrap();
        }

        // Check if fields match the schema
        assert!(verify_only_given_fields(&raw_fields, &schema).is_ok());
    }

    #[rstest]
    // Missing raw field
    #[case(
        vec![
            ("message", FieldType::Text),
            ("age", FieldType::Integer),
            ("is_cute", FieldType::Boolean),
        ],
        vec![
            ("spam", RawValue::Text("PANDA IS THE CUTEST!".into())),
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
            ("is_cute", RawValue::Boolean(false)),
            ("age", RawValue::Integer(41)),
            ("message", RawValue::Text("Hello, Mr. Handa!".into())),
            ("response", RawValue::Text("Good bye!".into())),
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
            ("age", RawValue::Float(41.34)),
        ],
        "field 'age' does not match schema: invalid field type 'float', expected 'int'",
    )]
    // Wrong name
    #[case(
        vec![
            ("age", FieldType::Integer),
        ],
        vec![
            ("rage", RawValue::Integer(100)),
        ],
        "unexpected fields found: 'rage'",
    )]
    fn wrong_only_given_fields(
        #[from(document_view_id)] schema_view_id: DocumentViewId,
        #[case] schema_fields: Vec<(&str, FieldType)>,
        #[case] fields: Vec<(&str, RawValue)>,
        #[case] expected: &str,
    ) {
        // Construct a schema
        let schema = Schema::new(
            &SchemaId::Application("zoo".to_owned(), schema_view_id),
            "Some schema description",
            schema_fields,
        )
        .unwrap();

        // Construct raw fields
        let mut raw_fields = RawFields::new();
        for (raw_field_name, raw_field_value) in fields {
            raw_fields.insert(&raw_field_name, raw_field_value).unwrap();
        }

        // Check if fields match the schema
        assert_eq!(
            verify_only_given_fields(&raw_fields, &schema)
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
            vec![("icecream", FieldType::Text), ("degree", FieldType::Float)],
        )
        .unwrap();

        // Construct raw fields
        let mut raw_fields = RawFields::new();
        raw_fields
            .insert("icecream", RawValue::Text("Almond".into()))
            .unwrap();
        raw_fields.insert("degree", RawValue::Float(6.12)).unwrap();

        // Construct expected operation fields
        let mut fields = OperationFields::new();
        fields
            .insert("icecream", OperationValue::Text("Almond".into()))
            .unwrap();
        fields
            .insert("degree", OperationValue::Float(6.12))
            .unwrap();

        // Verification methods should give us the validated operation fields
        assert_eq!(verify_all_fields(&raw_fields, &schema).unwrap(), fields);
        assert_eq!(
            verify_only_given_fields(&raw_fields, &schema).unwrap(),
            fields
        );
    }
}
