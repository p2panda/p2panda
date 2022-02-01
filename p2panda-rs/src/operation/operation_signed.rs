// SPDX-License-Identifier: AGPL-3.0-or-later

use std::hash::Hash as StdHash;

use crate::entry::{decode_entry, EntrySigned};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{
    AsOperation, Operation, OperationAction, OperationEncoded, OperationFields,
    OperationSignedError, OperationVersion,
};
use crate::Validate;

/// `OperationSigned` represents an operation, a data change, that has been published as part of
/// an [crate::entry::Entry]. That entry's hash identifies this operation and therefore
/// is called the *operation id*. This struct also contains the operation itself as a
/// plain [`Operation`] instance.
#[derive(Debug, Clone, Eq, PartialEq, StdHash)]
pub struct OperationSigned {
    /// The hash of this operation's entry.
    operation_id: Hash,

    /// The public key of the [`Author`] who published this operation.
    public_key: Author,

    /// The actual [`Operation`] this struct wraps.
    operation: Operation,
}

impl OperationSigned {
    /// Returns a new `OperationSigned` instance.
    pub fn new(
        entry_encoded: &EntrySigned,
        operation_encoded: &OperationEncoded,
    ) -> Result<Self, OperationSignedError> {
        let operation = Operation::from(operation_encoded);

        // This validates that the entry and operation are correctly matching
        decode_entry(entry_encoded, Some(operation_encoded))?;

        let operation_signed = Self {
            operation_id: entry_encoded.hash(),
            public_key: entry_encoded.author(),
            operation,
        };

        operation_signed.validate()?;

        Ok(operation_signed)
    }

    /// Returns the identifier for this operation, which is equal to the hash of the entry it
    /// was published with.
    pub fn operation_id(&self) -> &Hash {
        &self.operation_id
    }

    /// Returns the public key of the author of this operation.
    pub fn public_key(&self) -> &Author {
        &self.public_key
    }

    /// Returns the wrapped [`Operation`].
    pub fn operation(&self) -> &Operation {
        &self.operation
    }
}

impl AsOperation for OperationSigned {
    /// Returns action type of operation.
    fn action(&self) -> OperationAction {
        self.operation.action().to_owned()
    }

    /// Returns version of operation.
    fn version(&self) -> OperationVersion {
        self.operation.version().to_owned()
    }

    /// Returns schema of operation.
    fn schema(&self) -> Hash {
        self.operation.schema()
    }

    /// Returns data fields of operation.
    fn fields(&self) -> Option<OperationFields> {
        self.operation.fields()
    }

    /// Returns vector of previous operations.
    fn previous_operations(&self) -> Option<Vec<Hash>> {
        self.operation.previous_operations()
    }
}

impl Validate for OperationSigned {
    type Error = OperationSignedError;

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
    use crate::operation::{AsOperation, OperationEncoded, OperationValue};
    use crate::test_utils::fixtures::defaults::hash;
    use crate::test_utils::fixtures::templates::{
        all_signed_operation_types, implements_as_operation,
    };
    use crate::test_utils::fixtures::{
        create_operation, defaults, entry_signed_encoded, fields, operation_encoded,
    };
    use crate::Validate;

    use super::OperationSigned;

    #[rstest]
    #[should_panic]
    #[case(operation_encoded(create_operation(hash(), fields(vec![("message", OperationValue::Text("Not the right message".to_string()))]))))]
    #[case(operation_encoded(defaults::create_operation()))]
    fn create_operation_signed(
        entry_signed_encoded: EntrySigned,
        #[case] operation_encoded: OperationEncoded,
    ) {
        let operation_signed = OperationSigned::new(&entry_signed_encoded, &operation_encoded);
        assert!(operation_signed.is_ok())
    }

    #[apply(all_signed_operation_types)]
    fn only_some_operations_should_contain_fields(#[case] operation_signed: OperationSigned) {
        if operation_signed.is_create() {
            assert!(operation_signed.operation().fields().is_some());
        }

        if operation_signed.is_update() {
            assert!(operation_signed.operation().fields().is_some());
        }

        if operation_signed.is_delete() {
            assert!(operation_signed.operation().fields().is_none());
        }
    }

    #[apply(all_signed_operation_types)]
    fn operations_should_validate(#[case] operation_signed: OperationSigned) {
        assert!(operation_signed.operation().validate().is_ok());
        assert!(operation_signed.validate().is_ok())
    }

    #[apply(all_signed_operation_types)]
    fn trait_methods_should_match(#[case] operation_signed: OperationSigned) {
        let operation = operation_signed.operation();
        assert_eq!(operation_signed.fields(), operation.fields());
        assert_eq!(operation_signed.action(), operation.action());
        assert_eq!(operation_signed.version(), operation.version());
        assert_eq!(operation_signed.schema(), operation.schema());
        assert_eq!(
            operation_signed.previous_operations(),
            operation.previous_operations()
        );
    }

    #[apply(implements_as_operation)]
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

    #[apply(all_signed_operation_types)]
    fn it_hashes(#[case] operation_signed: OperationSigned) {
        let mut hash_map = HashMap::new();
        let key_value = "Value identified by a hash".to_string();
        hash_map.insert(&operation_signed, key_value.clone());
        let key_value_retrieved = hash_map.get(&operation_signed).unwrap().to_owned();
        assert_eq!(key_value, key_value_retrieved)
    }
}
