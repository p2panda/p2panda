// SPDX-License-Identifier: AGPL-3.0-or-later

use std::hash::Hash as StdHash;

use crate::entry::{decode_entry, EntrySigned};
use crate::identity::Author;
use crate::operation::{
    AsOperation, Operation, OperationAction, OperationEncoded, OperationFields, OperationVersion,
    OperationWithMetaError,
};
use crate::schema::SchemaId;
use crate::Validate;

use super::OperationId;

/// Wrapper struct containing an operation, its operation id, and the public key of its
/// author.
#[derive(Debug, Clone, Eq, PartialEq, StdHash)]
pub struct OperationWithMeta {
    /// The hash of this operation's entry.
    operation_id: OperationId,

    /// The public key of the author who published this operation.
    public_key: Author,

    /// The actual operation this struct wraps.
    operation: Operation,
}

impl OperationWithMeta {
    /// Returns a new `OperationWithMeta` instance.
    pub fn new(
        public_key: &Author,
        operation_id: &OperationId,
        operation: &Operation,
    ) -> Result<Self, OperationWithMetaError> {
        let operation_with_meta = Self {
            public_key: public_key.clone(),
            operation_id: operation_id.clone(),
            operation: operation.clone(),
        };

        operation_with_meta.validate()?;

        Ok(operation_with_meta)
    }

    /// Returns a new `OperationWithMeta` instance constructed from an `EntrySigned`
    /// and an `OperationEncoded`. This constructor method validates that the passed
    /// operation matches the one oncoded in the passed signed entry.
    pub fn new_from_entry(
        entry_encoded: &EntrySigned,
        operation_encoded: &OperationEncoded,
    ) -> Result<Self, OperationWithMetaError> {
        let operation = Operation::from(operation_encoded);

        // This validates that the entry and operation are correctly matching
        decode_entry(entry_encoded, Some(operation_encoded))?;

        let operation_with_meta = Self {
            operation_id: entry_encoded.hash().into(),
            public_key: entry_encoded.author(),
            operation,
        };

        operation_with_meta.validate()?;

        Ok(operation_with_meta)
    }

    /// Returns the identifier for this operation.
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns the public key of the author of this operation.
    pub fn public_key(&self) -> &Author {
        &self.public_key
    }

    /// Returns the wrapped operation.
    pub fn operation(&self) -> &Operation {
        &self.operation
    }
}

impl AsOperation for OperationWithMeta {
    /// Returns action type of operation.
    fn action(&self) -> OperationAction {
        self.operation.action().to_owned()
    }

    /// Returns version of operation.
    fn version(&self) -> OperationVersion {
        self.operation.version().to_owned()
    }

    /// Returns schema of operation.
    fn schema(&self) -> SchemaId {
        self.operation.schema()
    }

    /// Returns data fields of operation.
    fn fields(&self) -> Option<OperationFields> {
        self.operation.fields()
    }

    /// Returns vector of previous operations.
    fn previous_operations(&self) -> Option<Vec<OperationId>> {
        self.operation.previous_operations()
    }
}

impl Validate for OperationWithMeta {
    type Error = OperationWithMetaError;

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
    use crate::identity::Author;
    use crate::operation::{
        AsOperation, Operation, OperationEncoded, OperationId, OperationValue, OperationWithMeta,
    };
    use crate::test_utils::fixtures::defaults::schema;
    use crate::test_utils::fixtures::templates::{
        all_meta_operation_types, implements_as_operation,
    };
    use crate::test_utils::fixtures::{
        create_operation, defaults, entry_signed_encoded, fields, operation_encoded, operation_id,
        public_key,
    };
    use crate::Validate;

    #[rstest]
    #[should_panic]
    #[case(operation_encoded(create_operation(schema(), fields(vec![("message", OperationValue::Text("Not the right message".to_string()))]))))]
    #[case(operation_encoded(defaults::create_operation()))]
    fn create_operation_with_meta(
        entry_signed_encoded: EntrySigned,
        #[case] operation_encoded: OperationEncoded,
    ) {
        let operation_with_meta =
            OperationWithMeta::new_from_entry(&entry_signed_encoded, &operation_encoded);
        assert!(operation_with_meta.is_ok())
    }

    #[rstest]
    fn new_operation_not_from_entry(
        public_key: Author,
        operation_id: OperationId,
        #[from(create_operation)] operation: Operation,
    ) {
        let operation_with_meta = OperationWithMeta::new(&public_key, &operation_id, &operation);
        assert!(operation_with_meta.is_ok());
        let operation_with_meta = operation_with_meta.unwrap();
        assert_eq!(operation_with_meta.fields(), operation.fields());
        assert_eq!(operation_with_meta.action(), operation.action());
        assert_eq!(operation_with_meta.version(), operation.version());
        assert_eq!(operation_with_meta.schema(), operation.schema());
        assert_eq!(
            operation_with_meta.previous_operations(),
            operation.previous_operations()
        );
        assert_eq!(operation_with_meta.public_key(), &public_key);
        assert_eq!(operation_with_meta.operation_id(), &operation_id);
    }

    #[apply(all_meta_operation_types)]
    fn only_some_operations_should_contain_fields(#[case] operation_with_meta: OperationWithMeta) {
        if operation_with_meta.is_create() {
            assert!(operation_with_meta.operation().fields().is_some());
        }

        if operation_with_meta.is_update() {
            assert!(operation_with_meta.operation().fields().is_some());
        }

        if operation_with_meta.is_delete() {
            assert!(operation_with_meta.operation().fields().is_none());
        }
    }

    #[apply(all_meta_operation_types)]
    fn operations_should_validate(#[case] operation_with_meta: OperationWithMeta) {
        assert!(operation_with_meta.operation().validate().is_ok());
        assert!(operation_with_meta.validate().is_ok())
    }

    #[apply(all_meta_operation_types)]
    fn trait_methods_should_match(#[case] operation_with_meta: OperationWithMeta) {
        let operation = operation_with_meta.operation();
        assert_eq!(operation_with_meta.fields(), operation.fields());
        assert_eq!(operation_with_meta.action(), operation.action());
        assert_eq!(operation_with_meta.version(), operation.version());
        assert_eq!(operation_with_meta.schema(), operation.schema());
        assert_eq!(
            operation_with_meta.previous_operations(),
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

    #[apply(all_meta_operation_types)]
    fn it_hashes(#[case] operation_with_meta: OperationWithMeta) {
        let mut hash_map = HashMap::new();
        let key_value = "Value identified by a hash".to_string();
        hash_map.insert(&operation_with_meta, key_value.clone());
        let key_value_retrieved = hash_map.get(&operation_with_meta).unwrap().to_owned();
        assert_eq!(key_value, key_value_retrieved)
    }
}
