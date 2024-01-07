// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::api::ValidationError;
use crate::document::DocumentId;
use crate::hash::{Hash, HashId};
use crate::operation::body::plain::PlainOperation;
use crate::operation::body::traits::Schematic;
use crate::operation::body::Body;
use crate::operation::traits::{Actionable, Capable, Identifiable, Timestamped};
use crate::operation::OperationAction;
use crate::schema::validate::{validate_all_fields, validate_only_given_fields};
use crate::schema::{Schema, SchemaId};

use super::error::ValidatePlainOperationError;

/// Checks the fields and format of an operation against a schema.
pub fn validate_plain_operation(
    action: &OperationAction,
    plain_operation: &PlainOperation,
    schema: &Schema,
) -> Result<Body, ValidatePlainOperationError> {
    let fields = match (action, plain_operation.plain_fields()) {
        (OperationAction::Create, Some(fields)) => {
            validate_all_fields(&fields, schema).map(|fields| Some(fields))
        }
        (OperationAction::Update, Some(fields)) => {
            validate_only_given_fields(&fields, schema).map(|fields| Some(fields))
        }
        (OperationAction::Delete, None) => Ok(None),
        (OperationAction::Delete, Some(_)) => {
            return Err(ValidatePlainOperationError::UnexpectedFields)
        }
        (OperationAction::Create | OperationAction::Update, None) => {
            return Err(ValidatePlainOperationError::ExpectedFields)
        }
    }?;

    Ok(Body(fields))
}

pub fn validate_previous(
    operation: &(impl Identifiable + Actionable + Schematic + Capable + Timestamped),
    previous_schema_id: &SchemaId,
    previous_document_id: &DocumentId,
    previous_depth: u64,
    previous_timestamp: u128,
) -> Result<(), ValidationError> {
    if operation.schema_id() != previous_schema_id {
        return Err(ValidationError::MismathingSchemaInPrevious(
            operation.id().clone(),
            previous_schema_id.clone(),
            operation.schema_id().clone(),
        )
        .into());
    }

    if operation.document_id() != previous_document_id.clone() {
        return Err(ValidationError::MismatchingDocumentIdInPrevious(
            operation.id().clone(),
            previous_document_id.clone(),
            operation.document_id(),
        )
        .into());
    }

    if operation.depth() <= previous_depth {
        return Err(ValidationError::DepthLessThanPrevious(
            operation.id().clone(),
            operation.depth(),
        )
        .into());
    }

    // timestamp can be equal to previous timestamp.
    if operation.timestamp() < previous_timestamp {
        return Err(ValidationError::TimestampLessThanPrevious(
            operation.id().clone(),
            operation.timestamp(),
        )
        .into());
    }
    Ok(())
}

pub fn validate_backlink(
    operation: &(impl Identifiable + Capable + Timestamped),
    claimed_backlink: &Hash,
    backlink_hash: &Hash,
    backlink_depth: u64,
    backlink_timestamp: u128,
) -> Result<(), ValidationError> {
    if claimed_backlink != backlink_hash {
        return Err(ValidationError::IncorrectBacklink(
            operation.id().as_hash().clone(),
            operation.public_key().clone(),
            operation.document_id(),
            backlink_hash.clone(),
        )
        .into());
    }

    if operation.timestamp() < backlink_timestamp {
        return Err(ValidationError::TimestampLessThanBacklink(
            operation.id().clone(),
            operation.timestamp(),
        )
        .into());
    }

    if operation.depth() <= backlink_depth {
        return Err(ValidationError::DepthLessThanBacklink(
            operation.id().clone(),
            operation.depth(),
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::api::error::ValidatePlainOperationError;
    use crate::api::validate_plain_operation;
    use crate::document::{DocumentId, DocumentViewId};
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::body::plain::{PlainFields, PlainOperation, PlainValue};
    use crate::operation::header::HeaderAction;
    use crate::operation::{OperationAction, OperationBuilder};
    use crate::schema::{FieldType, Schema};
    use crate::test_utils::constants::test_fields;
    use crate::test_utils::fixtures::{document_id, document_view_id, hash, key_pair, schema};

    const TIMESTAMP: u128 = 17037976940000000;

    #[rstest]
    fn validate_plain_operations_pass(
        key_pair: KeyPair,
        schema: Schema,
        document_id: DocumentId,
        #[from(document_view_id)] previous: DocumentViewId,
        #[from(hash)] backlink: Hash,
    ) {
        let create_operation = OperationBuilder::new(schema.id(), TIMESTAMP)
            .fields(&test_fields())
            .sign(&key_pair)
            .unwrap();

        let plain_operation: PlainOperation = create_operation.body().into();

        assert!(
            validate_plain_operation(&OperationAction::Create, &plain_operation, &schema).is_ok()
        );

        let update_operation = OperationBuilder::new(schema.id(), TIMESTAMP)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&previous)
            .depth(1)
            // Update just one field
            .fields(&[test_fields().first().unwrap().to_owned()])
            .sign(&key_pair)
            .unwrap();

        let plain_operation: PlainOperation = update_operation.body().into();

        assert!(
            validate_plain_operation(&OperationAction::Update, &plain_operation, &schema).is_ok()
        );

        let delete_operation = OperationBuilder::new(schema.id(), TIMESTAMP)
            .document_id(&document_id)
            .action(HeaderAction::Delete)
            .backlink(&backlink)
            .previous(&previous)
            .depth(1)
            .sign(&key_pair)
            .unwrap();

        let plain_operation: PlainOperation = delete_operation.body().into();

        assert!(
            validate_plain_operation(&OperationAction::Delete, &plain_operation, &schema).is_ok()
        );
    }

    #[rstest]
    fn validate_plain_operations_failure(
        #[with(vec![("name".to_string(), FieldType::String), ("age".to_string(), FieldType::Integer)])]
        schema: Schema,
    ) {
        let mut plain_fields = PlainFields::new();
        plain_fields
            .insert("name", PlainValue::String("panda".to_string()))
            .unwrap();
        plain_fields.insert("age", PlainValue::Integer(12)).unwrap();

        // CREATE and UPDATE operations must have fields.
        let plain_operation = PlainOperation(None);
        let error = validate_plain_operation(&OperationAction::Create, &plain_operation, &schema)
            .unwrap_err();
        assert!(matches!(error, ValidatePlainOperationError::ExpectedFields));

        let error = validate_plain_operation(&OperationAction::Update, &plain_operation, &schema)
            .unwrap_err();
        assert!(matches!(error, ValidatePlainOperationError::ExpectedFields));

        // DELETE operations must not have fields.
        let plain_operation = PlainOperation(Some(plain_fields));
        let error = validate_plain_operation(&OperationAction::Delete, &plain_operation, &schema)
            .unwrap_err();
        assert!(matches!(
            error,
            ValidatePlainOperationError::UnexpectedFields
        ));

        // Errors occurring when validating operation fields against schema bubble up.
        let mut wrong_plain_fields = PlainFields::new();
        wrong_plain_fields
            .insert("name", PlainValue::String("panda".to_string()))
            .unwrap();
        wrong_plain_fields
            .insert("height", PlainValue::Float(187.89))
            .unwrap();

        let wrong_plain_operation = PlainOperation(Some(wrong_plain_fields.clone()));
        let error =
            validate_plain_operation(&OperationAction::Create, &wrong_plain_operation, &schema)
                .unwrap_err();
        assert!(matches!(
            error,
            ValidatePlainOperationError::ValidateFieldsError(_)
        ));
    }
}
