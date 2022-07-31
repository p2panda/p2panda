// SPDX-License-Identifier: AGPL-3.0-or-later

use std::hash::Hash as StdHash;

use crate::entry::{decode_entry, EntrySigned};
use crate::identity::Author;
use crate::operation::{AsVerifiedOperation, Operation, OperationEncoded, VerifiedOperationError};
use crate::Validate;

use super::OperationId;

/// An operation which has been encoded and published on a signed entry.
///
/// Contains the values of an operation as well as its author and id. This
/// [operation id][OperationId] is only available on [`VerifiedOperation`] and not on
/// [`Operation`] because it is derived from the hash of the signed entry an operation is encoded
/// on.
#[derive(Debug, Clone, Eq, PartialEq, StdHash)]
pub struct VerifiedOperation {
    /// The hash of the entry this operation was published with.
    operation_id: OperationId,

    /// The public key of the author who published this operation.
    public_key: Author,

    /// The actual operation this struct wraps.
    operation: Operation,
}

impl AsVerifiedOperation for VerifiedOperation {
    type VerifiedOperationError = VerifiedOperationError;

    /// Returns a new `VerifiedOperation` instance.
    ///
    /// Use `VerifiedOperation::new_from_entry()` instead if you want to validate that the operation
    /// was signed by this public key.
    fn new(
        public_key: &Author,
        operation_id: &OperationId,
        operation: &Operation,
    ) -> Result<Self, VerifiedOperationError> {
        let verified_operation = Self {
            public_key: public_key.clone(),
            operation_id: operation_id.clone(),
            operation: operation.clone(),
        };

        verified_operation.validate()?;

        Ok(verified_operation)
    }

    /// Returns a new `VerifiedOperation` instance constructed from an `EntrySigned`
    /// and an `OperationEncoded`.
    ///
    /// This constructor verifies that the passed operation matches the one encoded
    /// in the passed signed entry.
    fn new_from_entry(
        entry_encoded: &EntrySigned,
        operation_encoded: &OperationEncoded,
    ) -> Result<Self, VerifiedOperationError> {
        let operation = Operation::from(operation_encoded);

        // This verifies that the entry and operation are correctly matching.
        decode_entry(entry_encoded, Some(operation_encoded))?;

        let verified_operation = Self {
            operation_id: entry_encoded.hash().into(),
            public_key: entry_encoded.author(),
            operation,
        };

        verified_operation.validate()?;

        Ok(verified_operation)
    }
    /// Returns the identifier for this operation.
    fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns the public key of the author of this operation.
    fn public_key(&self) -> &Author {
        &self.public_key
    }

    /// Returns the wrapped operation.
    fn operation(&self) -> &Operation {
        &self.operation
    }
}

#[cfg(any(feature = "testing", test))]
impl VerifiedOperation {
    /// Create a verified operation from it's unverified parts for testing.
    pub fn new_test_operation(
        id: &OperationId,
        public_key: &Author,
        operation: &Operation,
    ) -> Self {
        Self {
            operation_id: id.clone(),
            public_key: public_key.clone(),
            operation: operation.clone(),
        }
    }
}

impl Validate for VerifiedOperation {
    type Error = VerifiedOperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.operation.validate()?;
        self.public_key.validate()?;
        self.operation_id.validate()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::entry::EntrySigned;
    use crate::identity::{Author, KeyPair};
    use crate::operation::{
        AsOperation, AsVerifiedOperation, Operation, OperationEncoded, OperationId, OperationValue,
        VerifiedOperation,
    };
    use crate::test_utils::constants::{test_fields, SCHEMA_ID};
    use crate::test_utils::fixtures::{
        entry_signed_encoded, key_pair, operation, operation_encoded, operation_fields,
        operation_id,
    };
    use crate::test_utils::templates::{
        legacy_implements_as_operation, legacy_many_verified_operations,
    };
    use crate::Validate;

    #[rstest]
    #[case(operation_encoded(Some(operation_fields(test_fields())), None, Some(SCHEMA_ID.parse().unwrap())))]
    #[should_panic]
    #[case(operation_encoded(Some(operation_fields(vec![("message", OperationValue::Text("Not the right message".to_string()))])), None, Some(SCHEMA_ID.parse().unwrap())))]
    fn create_verified_operation(
        entry_signed_encoded: EntrySigned,
        #[case] operation_encoded: OperationEncoded,
    ) {
        let verified_operation =
            VerifiedOperation::new_from_entry(&entry_signed_encoded, &operation_encoded);
        assert!(verified_operation.is_ok())
    }

    #[rstest]
    fn new_operation_not_from_entry(
        key_pair: KeyPair,
        operation_id: OperationId,
        #[from(operation)] operation: Operation,
    ) {
        let author = Author::from(key_pair.public_key());
        let verified_operation = VerifiedOperation::new(&author, &operation_id, &operation);
        assert!(verified_operation.is_ok());
        let verified_operation = verified_operation.unwrap();
        assert_eq!(verified_operation.fields(), operation.fields());
        assert_eq!(verified_operation.action(), operation.action());
        assert_eq!(verified_operation.version(), operation.version());
        assert_eq!(verified_operation.schema(), operation.schema());
        assert_eq!(
            verified_operation.previous_operations(),
            operation.previous_operations()
        );
        assert_eq!(verified_operation.public_key(), &author);
        assert_eq!(verified_operation.operation_id(), &operation_id);
    }

    #[apply(legacy_many_verified_operations)]
    fn only_some_operations_should_contain_fields(#[case] verified_operation: VerifiedOperation) {
        if verified_operation.is_create() {
            assert!(verified_operation.operation().fields().is_some());
        }

        if verified_operation.is_update() {
            assert!(verified_operation.operation().fields().is_some());
        }

        if verified_operation.is_delete() {
            assert!(verified_operation.operation().fields().is_none());
        }
    }

    #[apply(legacy_many_verified_operations)]
    fn operations_should_validate(#[case] verified_operation: VerifiedOperation) {
        assert!(verified_operation.operation().validate().is_ok());
        assert!(verified_operation.validate().is_ok())
    }

    #[apply(legacy_many_verified_operations)]
    fn trait_methods_should_match(#[case] verified_operation: VerifiedOperation) {
        let operation = verified_operation.operation();
        assert_eq!(verified_operation.fields(), operation.fields());
        assert_eq!(verified_operation.action(), operation.action());
        assert_eq!(verified_operation.version(), operation.version());
        assert_eq!(verified_operation.schema(), operation.schema());
        assert_eq!(
            verified_operation.previous_operations(),
            operation.previous_operations()
        );
    }

    #[apply(legacy_implements_as_operation)]
    fn operation_has_same_trait_methods(#[case] operation: impl AsOperation) {
        operation.is_create();
        operation.is_update();
        operation.fields();
        operation.action();
        operation.version();
        operation.schema();
        operation.previous_operations();
        operation.has_fields();
        operation.has_previous_operations();
    }

    #[apply(legacy_many_verified_operations)]
    fn it_hashes(#[case] verified_operation: VerifiedOperation) {
        let mut hash_map = HashMap::new();
        let key_value = "Value identified by a hash".to_string();
        hash_map.insert(&verified_operation, key_value.clone());
        let key_value_retrieved = hash_map.get(&verified_operation).unwrap().to_owned();
        assert_eq!(key_value, key_value_retrieved)
    }
}
