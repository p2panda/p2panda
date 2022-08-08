// SPDX-License-Identifier: AGPL-3.0-or-later

use std::hash::Hash as StdHash;

use crate::entry::Entry;
use crate::identity::Author;
use crate::operation::traits::AsVerifiedOperation;
use crate::operation::{Operation, OperationId};

/// An operation which has been encoded and published on a signed entry.
///
/// Contains the values of an operation as well as its author and id. This
/// [operation id][OperationId] is only available on [`VerifiedOperation`] and not on
/// [`Operation`] because it is derived from the hash of the signed entry an operation is encoded
/// on.
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

impl Eq for VerifiedOperation {}

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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::operation::traits::{AsOperation, AsVerifiedOperation};
    use crate::test_utils::constants::test_fields;
    use crate::test_utils::fixtures::verified_operation;
    use crate::test_utils::templates::{implements_as_operation, many_verified_operations};

    use super::VerifiedOperation;

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
    fn trait_methods_should_match(#[case] verified_operation: VerifiedOperation) {
        let operation = verified_operation.operation();
        assert_eq!(verified_operation.fields(), operation.fields());
        assert_eq!(verified_operation.action(), operation.action());
        assert_eq!(verified_operation.version(), operation.version());
        assert_eq!(verified_operation.schema_id(), operation.schema_id());
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
        operation.schema_id();
        operation.previous_operations();
        operation.has_fields();
        operation.has_previous_operations();
    }

    #[rstest]
    fn it_hashes(verified_operation: VerifiedOperation) {
        // Use verified operation as hash map key
        let mut hash_map = HashMap::new();
        let key_value = "Value identified by a hash".to_string();
        hash_map.insert(&verified_operation, key_value.clone());

        // Obtain value from hash map
        let key_value_retrieved = hash_map.get(&verified_operation).unwrap().to_owned();
        assert_eq!(key_value, key_value_retrieved)
    }
}
