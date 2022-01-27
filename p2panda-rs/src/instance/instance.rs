// SPDX-License-Identifier: AGPL-3.0-or-later

//! Types and methods for deriving and maintaining materialised documents.
use std::collections::btree_map::Iter as BTreeMapIter;
use std::collections::BTreeMap;
use std::convert::TryFrom;

use crate::instance::error::InstanceError;
use crate::operation::{AsOperation, Operation, OperationValue, OperationWithMeta};

/// The materialised view of a reduced collection of `Operations` describing a document.
#[derive(Debug, PartialEq, Default)]
pub struct Instance(BTreeMap<String, OperationValue>);

impl Instance {
    /// Returns a new `Instance`.
    fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Update this `Instance` from an UPDATE `Operation`.
    pub fn apply_update<T: AsOperation>(&mut self, operation: T) -> Result<(), InstanceError> {
        if !operation.is_update() {
            return Err(InstanceError::NotUpdateOperation);
        };

        let fields = operation.fields();

        if let Some(fields) = fields {
            for (key, value) in fields.iter() {
                self.0.insert(key.to_string(), value.to_owned());
            }
        }

        Ok(())
    }

    /// Returns a vector containing the keys of this instance.
    pub fn keys(&self) -> Vec<String> {
        self.0.clone().into_keys().collect::<Vec<String>>()
    }

    /// Returns an iterator of existing instance fields.
    pub fn iter(&self) -> BTreeMapIter<String, OperationValue> {
        self.0.iter()
    }

    /// Returns the number of fields on this instance.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl TryFrom<Operation> for Instance {
    type Error = InstanceError;

    fn try_from(operation: Operation) -> Result<Instance, InstanceError> {
        if !operation.is_create() {
            return Err(InstanceError::NotCreateOperation);
        };

        let mut instance: Instance = Instance::new();
        let fields = operation.fields();

        if let Some(fields) = fields {
            for (key, value) in fields.iter() {
                instance.0.insert(key.to_string(), value.to_owned());
            }
        }

        Ok(instance)
    }
}

impl TryFrom<OperationWithMeta> for Instance {
    type Error = InstanceError;

    fn try_from(operation: OperationWithMeta) -> Result<Instance, InstanceError> {
        if !operation.is_create() {
            return Err(InstanceError::NotCreateOperation);
        };

        let mut instance: Instance = Instance::new();
        let fields = operation.fields();

        if let Some(fields) = fields {
            for (key, value) in fields.iter() {
                instance.0.insert(key.to_string(), value.to_owned());
            }
        }

        Ok(instance)
    }
}

impl From<BTreeMap<String, OperationValue>> for Instance {
    fn from(map: BTreeMap<String, OperationValue>) -> Self {
        Self(map)
    }
}

// @TODO: This currently makes sure the wasm tests work as cddl does not have any wasm support
// (yet). Remove this with: https://github.com/p2panda/p2panda/issues/99
#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use std::convert::{TryFrom, TryInto};

    use rstest::rstest;

    use crate::hash::Hash;
    use crate::operation::{AsOperation, Operation, OperationValue};
    use crate::schema::Schema;
    use crate::test_utils::fixtures::{
        create_operation, delete_operation, fields, hash, schema, update_operation,
    };

    use super::Instance;

    #[rstest]
    fn encode_and_decode(schema: Hash) {
        let operation = create_operation(
            schema,
            fields(vec![
                ("username", OperationValue::Text("bubu".to_owned())),
                ("height", OperationValue::Float(3.5)),
                ("age", OperationValue::Integer(28)),
                ("is_admin", OperationValue::Boolean(false)),
                (
                    "profile_picture",
                    OperationValue::Relation(Hash::new_from_bytes(vec![1, 2, 3]).unwrap()),
                ),
            ]),
        );

        // Convert a CREATE `Operation` into an `Instance`
        let instance: Instance = operation.try_into().unwrap();

        assert_eq!(
            instance.keys(),
            vec!["age", "height", "is_admin", "profile_picture", "username"]
        )
    }

    #[rstest]
    fn try_from_operation(
        create_operation: Operation,
        update_operation: Operation,
        delete_operation: Operation,
    ) {
        // Convert a CREATE `Operation` into an `Instance`
        let instance: Instance = create_operation.clone().try_into().unwrap();

        let mut expected_instance = Instance::new();
        expected_instance.0.insert(
            "message".to_string(),
            create_operation
                .fields()
                .unwrap()
                .get("message")
                .unwrap()
                .to_owned(),
        );
        assert_eq!(instance, expected_instance);

        // Convert an UPDATE or DELETE `Operation` into an `Instance`
        let instance_1 = Instance::try_from(update_operation);
        let instance_2 = Instance::try_from(delete_operation);

        assert!(instance_1.is_err());
        assert!(instance_2.is_err());
    }

    #[rstest]
    pub fn update(create_operation: Operation, update_operation: Operation) {
        let mut chat_instance = Instance::try_from(create_operation.clone()).unwrap();
        chat_instance
            .apply_update(update_operation.clone())
            .unwrap();

        let mut exp_chat_instance = Instance::new();

        exp_chat_instance.0.insert(
            "message".to_string(),
            create_operation
                .fields()
                .unwrap()
                .get("message")
                .unwrap()
                .to_owned(),
        );

        exp_chat_instance.0.insert(
            "message".to_string(),
            update_operation
                .fields()
                .unwrap()
                .get("message")
                .unwrap()
                .to_owned(),
        );

        assert_eq!(chat_instance, exp_chat_instance)
    }

    #[rstest]
    pub fn create_from_schema(#[from(hash)] schema_hash: Hash, create_operation: Operation) {
        // Instantiate "person" schema from CDDL string
        let chat_schema_definition = "
            chat = { (
                message: { type: \"str\", value: tstr }
            ) }
        ";

        let chat = Schema::new(&schema_hash, &chat_schema_definition.to_string()).unwrap();
        let chat_instance = chat.instance_from_create(create_operation.clone()).unwrap();

        let mut exp_chat_instance = Instance::new();
        exp_chat_instance.0.insert(
            "message".to_string(),
            create_operation
                .fields()
                .unwrap()
                .get("message")
                .unwrap()
                .to_owned(),
        );

        assert_eq!(chat_instance, exp_chat_instance)
    }
}
