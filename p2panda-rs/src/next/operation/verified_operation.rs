// SPDX-License-Identifier: AGPL-3.0-or-later

use std::hash::Hash as StdHash;

use crate::identity::Author;
use crate::next::entry::Entry;
use crate::next::operation::traits::AsVerifiedOperation;
use crate::next::operation::{Operation, OperationId};

/// An operation which has been encoded and published on a signed entry.
///
/// Contains the values of an operation as well as its author and id. This
/// [operation id][OperationId] is only available on [`VerifiedOperation`] and not on
/// [`Operation`] because it is derived from the hash of the signed entry an operation is encoded
/// on.
// @TODO: Fix pub(crate) visibility
#[derive(Debug, Clone)]
pub struct VerifiedOperation {
    /// Identifier of the operation.
    pub(crate) operation_id: OperationId,

    /// Operation, which is the payload of the entry.
    pub(crate) operation: Operation,

    /// Entry which was used to publish this operation.
    pub(crate) entry: Entry,
}

impl VerifiedOperation {
    /// Returns the entry related to this operation.
    pub fn entry(&self) -> &Entry {
        &self.entry
    }
}

impl AsVerifiedOperation for VerifiedOperation {
    /// Returns the identifier for this operation.
    fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns the wrapped operation.
    fn operation(&self) -> &Operation {
        &self.operation
    }

    /// Returns the public key of the author of this operation.
    fn public_key(&self) -> &Author {
        self.entry.public_key()
    }
}

impl PartialEq for VerifiedOperation {
    fn eq(&self, other: &Self) -> bool {
        self.operation_id() == other.operation_id()
    }
}

impl StdHash for VerifiedOperation {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.operation_id().hash(state)
    }
}

#[cfg(test)]
impl VerifiedOperation {
    /// Create a verified operation from it's unverified parts for testing.
    pub fn new(entry: &Entry, operation: &Operation, operation_id: &OperationId) -> Self {
        Self {
            operation_id: operation_id.to_owned(),
            operation: operation.to_owned(),
            entry: entry.to_owned(),
        }
    }
}

// @TODO: Requires refactoring of entry fixtures
/* #[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::convert::TryFrom;

    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::identity::{Author, KeyPair};
    use crate::next::entry::EncodedEntry;
    use crate::next::operation::traits::{AsOperation, AsVerifiedOperation};
    use crate::next::operation::{
        Operation, OperationEncoded, OperationId, OperationValue, VerifiedOperation,
    };
    use crate::next::test_utils::constants::{test_fields, SCHEMA_ID};
    use crate::next::test_utils::fixtures::{
        entry_signed_encoded, key_pair, operation, operation_encoded, operation_fields,
        operation_id,
    };
    use crate::test_utils::fixtures::key_pair;
    use crate::test_utils::templates::{implements_as_operation, many_verified_operations};
    use crate::Validate;

    #[rstest]
    #[case(operation_encoded(Some(operation_fields(test_fields())), None, Some(SCHEMA_ID.parse().unwrap())))]
    #[should_panic]
    #[case(operation_encoded(Some(operation_fields(vec![("message", OperationValue::Text("Not the right message".to_string()))])), None, Some(SCHEMA_ID.parse().unwrap())))]
    fn create_verified_operation(
        entry_signed_encoded: EncodedEntry,
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
        let author = Author::try_from(*key_pair.public_key()).unwrap();
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

    #[apply(many_verified_operations)]
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

    #[apply(many_verified_operations)]
    fn operations_should_validate(#[case] verified_operation: VerifiedOperation) {
        assert!(verified_operation.operation().validate().is_ok());
        assert!(verified_operation.validate().is_ok())
    }

    #[apply(many_verified_operations)]
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

    fn it_hashes(verified_operation: VerifiedOperation) {
        let mut hash_map = HashMap::new();
        let key_value = "Value identified by a hash".to_string();
        hash_map.insert(&verified_operation, key_value.clone());
        let key_value_retrieved = hash_map.get(&verified_operation).unwrap().to_owned();
        assert_eq!(key_value, key_value_retrieved)
    }
} */
