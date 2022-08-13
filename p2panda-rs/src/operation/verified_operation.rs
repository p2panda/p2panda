// SPDX-License-Identifier: AGPL-3.0-or-later

use std::hash::Hash as StdHash;

use crate::document::DocumentViewId;
use crate::identity::Author;
use crate::operation::traits::{AsOperation, AsVerifiedOperation};
#[cfg(test)]
use crate::operation::Operation;
use crate::operation::{OperationAction, OperationFields, OperationId, OperationVersion};
use crate::schema::SchemaId;

/// An operation which has been encoded and published on a signed entry.
///
/// Contains the values of an operation as well as its author and id. This
/// [operation id][OperationId] is only available on [`VerifiedOperation`] and not on
/// [`Operation`] because it is derived from the hash of the signed entry an operation is encoded
/// on.
#[derive(Debug, Clone)]
pub struct VerifiedOperation {
    /// Identifier of the operation.
    pub(crate) id: OperationId,

    /// Version of this operation.
    pub(crate) version: OperationVersion,

    /// Action of this operation.
    pub(crate) action: OperationAction,

    /// Schema instance of this operation.
    pub(crate) schema_id: SchemaId,

    /// Previous operations field.
    pub(crate) previous_operations: Option<DocumentViewId>,

    /// Operation fields.
    pub(crate) fields: Option<OperationFields>,

    /// The public key of the key pair used to publish this operation.
    pub(crate) public_key: Author,
}

impl AsVerifiedOperation for VerifiedOperation {
    /// Returns the identifier for this operation.
    fn id(&self) -> &OperationId {
        &self.id
    }

    /// Returns the public key of the author of this operation.
    fn public_key(&self) -> &Author {
        &self.public_key
    }
}

impl AsOperation for VerifiedOperation {
    /// Returns action type of operation.
    fn action(&self) -> OperationAction {
        self.action.to_owned()
    }

    /// Returns schema if of operation.
    fn schema_id(&self) -> SchemaId {
        self.schema_id.to_owned()
    }

    /// Returns version of operation.
    fn version(&self) -> OperationVersion {
        self.version.to_owned()
    }

    /// Returns application data fields of operation.
    fn fields(&self) -> Option<OperationFields> {
        self.fields.clone()
    }

    /// Returns vector of this operation's previous operation ids
    fn previous_operations(&self) -> Option<DocumentViewId> {
        self.previous_operations.clone()
    }
}

impl Eq for VerifiedOperation {}

impl PartialEq for VerifiedOperation {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl StdHash for VerifiedOperation {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state)
    }
}

#[cfg(test)]
impl VerifiedOperation {
    /// Create a verified operation from it's unverified parts for testing.
    pub fn new(public_key: &Author, operation: &Operation, operation_id: &OperationId) -> Self {
        Self {
            id: operation_id.to_owned(),
            public_key: public_key.to_owned(),
            version: OperationVersion::V1,
            action: operation.action(),
            schema_id: operation.schema_id(),
            previous_operations: operation.previous_operations(),
            fields: operation.fields(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::operation::traits::AsOperation;
    use crate::test_utils::constants::test_fields;
    use crate::test_utils::fixtures::verified_operation;
    use crate::test_utils::templates::{implements_as_operation, many_verified_operations};

    use super::VerifiedOperation;

    #[apply(many_verified_operations)]
    fn only_some_operations_should_contain_fields(#[case] verified_operation: VerifiedOperation) {
        if verified_operation.is_create() {
            assert!(verified_operation.fields().is_some());
        }

        if verified_operation.is_update() {
            assert!(verified_operation.fields().is_some());
        }

        if verified_operation.is_delete() {
            assert!(verified_operation.fields().is_none());
        }
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
