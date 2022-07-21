// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentViewId;
use crate::operation::{
    Operation, OperationAction, OperationError, OperationFields, OperationValue, OperationVersion,
    PinnedRelation, PinnedRelationList, RawFields, RawOperation, RawValue, Relation, RelationList,
};
use crate::schema::{FieldName, FieldType, Schema};

pub fn verify_schema_and_convert(
    raw_operation: &RawOperation,
    schema: &Schema,
) -> Result<Operation, OperationError> {
    if raw_operation.version() != &OperationVersion::Default {
        // @TODO: Correct error type: Unknown operation version
        return Err(OperationError::EmptyFields);
    }

    match raw_operation.action() {
        OperationAction::Create => verify_create_operation(
            raw_operation.previous_operations(),
            raw_operation.fields(),
            &schema,
        ),
        OperationAction::Update => verify_update_operation(
            raw_operation.previous_operations(),
            raw_operation.fields(),
            &schema,
        ),
        OperationAction::Delete => verify_delete_operation(
            raw_operation.previous_operations(),
            raw_operation.fields(),
            &schema,
        ),
    }
}

fn verify_create_operation(
    raw_previous_operations: Option<&DocumentViewId>,
    raw_fields: Option<&RawFields>,
    schema: &Schema,
) -> Result<Operation, OperationError> {
    if raw_previous_operations.is_some() {
        // @TODO: Correct error type: Previous operations should not be set
        return Err(OperationError::EmptyFields);
    }

    let mut validated_fields = OperationFields::new();

    match raw_fields {
        Some(fields) => {
            let raw_fields_iter = fields.iter();

            for schema_field in schema.fields() {
                match raw_fields_iter.next() {
                    Some((raw_name, raw_value)) => {
                        let (validated_name, validated_value) =
                            verify_schema_field_and_convert((raw_name, raw_value), schema_field)?;
                        validated_fields.insert(&validated_name, validated_value);
                        Ok(())
                    }
                    None => {
                        // @TODO: Correct error type: Field x not given
                        Err(OperationError::EmptyFields)
                    }
                };
            }

            if fields.len() != schema.fields().len() {
                // @TODO: Correct error type: Too many fields
                return Err(OperationError::EmptyFields);
            }
        }
        None => {
            // @TODO: Correct error type: No fields given
            return Err(OperationError::EmptyFields);
        }
    };

    let operation = Operation::new_create(schema.id().to_owned(), validated_fields)?;
    Ok(operation)
}

fn verify_update_operation(
    raw_previous_operations: Option<&DocumentViewId>,
    raw_fields: Option<&RawFields>,
    schema: &Schema,
) -> Result<Operation, OperationError> {
    let mut validated_fields = OperationFields::new();

    match raw_fields {
        Some(fields) => {
            let checked_fields = 0;

            for schema_field in schema.fields() {
                match fields.find(schema_field.0) {
                    Some((raw_name, raw_value)) => {
                        let (validated_name, validated_value) =
                            verify_schema_field_and_convert((raw_name, raw_value), schema_field)?;
                        validated_fields.insert(&validated_name, validated_value);
                        checked_fields += 1
                    }
                    None => (),
                };
            }

            if checked_fields != fields.len() {
                // @TODO: Correct error type: Unknown fields
                return Err(OperationError::EmptyFields);
            }
        }
        None => {
            // @TODO: Correct error type: No fields given
            return Err(OperationError::EmptyFields);
        }
    };

    match raw_previous_operations {
        Some(previous_operations) => {
            let operation = Operation::new_update(
                schema.id().to_owned(),
                previous_operations.to_owned(),
                validated_fields,
            )?;

            Ok(operation)
        }
        None => {
            // @TODO: Correct error type: Previous operations should be set
            Err(OperationError::EmptyFields)
        }
    }
}

fn verify_delete_operation(
    raw_previous_operations: Option<&DocumentViewId>,
    raw_fields: Option<&RawFields>,
    schema: &Schema,
) -> Result<Operation, OperationError> {
    if raw_fields.is_some() {
        // @TODO: Correct error type: Fields should not be set
        return Err(OperationError::EmptyFields);
    }

    match raw_previous_operations {
        Some(previous_operations) => {
            let operation =
                Operation::new_delete(schema.id().to_owned(), previous_operations.to_owned())?;
            Ok(operation)
        }
        None => {
            // @TODO: Correct error type: Previous operations missing
            return Err(OperationError::EmptyFields);
        }
    }
}

fn verify_schema_field_and_convert<'a>(
    raw_field: (&'a FieldName, &RawValue),
    schema_field: (&FieldName, &FieldType),
) -> Result<(&'a FieldName, OperationValue), OperationError> {
    let validated_name = verify_field_name(raw_field.0, schema_field.0)?;
    let validated_value = verify_field_value(raw_field.1, schema_field.1)?;
    Ok((validated_name, validated_value))
}

fn verify_field_name<'a>(
    raw_field_name: &'a FieldName,
    schema_field_name: &FieldName,
) -> Result<&'a FieldName, OperationError> {
    if raw_field_name == schema_field_name {
        Ok(raw_field_name)
    } else {
        // @TODO: Correct error type: Invalid field name
        Err(OperationError::EmptyFields)
    }
}

fn verify_field_value(
    raw_value: &RawValue,
    schema_field_type: &FieldType,
) -> Result<OperationValue, OperationError> {
    match schema_field_type {
        FieldType::Boolean => {
            if let RawValue::Boolean(bool) = raw_value {
                Ok(OperationValue::Boolean(*bool))
            } else {
                // @TODO: Correct error type: Expected boolean
                Err(OperationError::EmptyFields)
            }
        }
        FieldType::Integer => {
            if let RawValue::Integer(int) = raw_value {
                Ok(OperationValue::Integer(*int))
            } else {
                // @TODO: Correct error type: Expected integer
                Err(OperationError::EmptyFields)
            }
        }
        FieldType::Float => {
            if let RawValue::Float(float) = raw_value {
                Ok(OperationValue::Float(*float))
            } else {
                // @TODO: Correct error type: Expected float
                Err(OperationError::EmptyFields)
            }
        }
        FieldType::Text => {
            if let RawValue::Text(str) = raw_value {
                Ok(OperationValue::Text(str.to_owned()))
            } else {
                // @TODO: Correct error type: Expected string
                Err(OperationError::EmptyFields)
            }
        }
        FieldType::Relation(_) => {
            // Note: This does NOT verify if the related document follows the given schema
            if let RawValue::Relation(document_id) = raw_value {
                Ok(OperationValue::Relation(Relation::new(
                    document_id.to_owned(),
                )))
            } else {
                // @TODO: Correct error type: Expected string for relation
                Err(OperationError::EmptyFields)
            }
        }
        FieldType::RelationList(_) => {
            // Note: This does NOT verify if the related documents follow the given schema
            if let RawValue::RelationList(document_ids) = raw_value {
                // @TODO: Is this sorted?
                Ok(OperationValue::RelationList(RelationList::new(
                    document_ids.to_owned(),
                )))
            } else {
                // @TODO: Correct error type: Expected array of strings for relation list
                Err(OperationError::EmptyFields)
            }
        }
        FieldType::PinnedRelation(_) => {
            // Note: This does NOT verify if the pinned document view follows the given schema
            if let RawValue::PinnedRelation(document_view_id) = raw_value {
                // @TODO: Is this sorted?
                Ok(OperationValue::PinnedRelation(PinnedRelation::new(
                    document_view_id.to_owned(),
                )))
            } else {
                // @TODO: Correct error type: Expected array of strings for pinned relation
                Err(OperationError::EmptyFields)
            }
        }
        FieldType::PinnedRelationList(_) => {
            // Note: This does NOT verify if the pinned document views follow the given schema
            if let RawValue::PinnedRelationList(document_view_ids) = raw_value {
                // @TODO: Is this sorted?
                Ok(OperationValue::PinnedRelationList(PinnedRelationList::new(
                    document_view_ids.to_owned(),
                )))
            } else {
                // @TODO: Correct error type: Expected array of strings for relation list
                Err(OperationError::EmptyFields)
            }
        }
    }
}
