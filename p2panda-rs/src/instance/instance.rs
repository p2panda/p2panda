// SPDX-License-Identifier: AGPL-3.0-or-later

//! Types and methods for deriving and maintaining materialised documents.
use std::{collections::HashMap, convert::TryFrom};

use crate::instance::error::InstanceError;
use crate::operation::{AsOperation, Operation, OperationValue};

/// The materialised view of a reduced collection of `Operations` describing a document.
#[derive(Debug, PartialEq, Default)]
pub struct Instance(HashMap<String, OperationValue>);

impl Instance {
    /// Returns a new `Instance`.
    fn new() -> Self {
        Self(HashMap::new())
    }

    /// Apply an UPDATE [`Operation`] on this `Instance`.
    pub fn update(&mut self, operation: Operation) -> Result<(), InstanceError> {
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

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use std::convert::{TryFrom, TryInto};

    use rstest::rstest;

    use crate::hash::Hash;
    use crate::operation::{AsOperation, Operation};
    use crate::schema::Schema;
    use crate::test_utils::fixtures::{create_operation, delete_operation, hash, update_operation};

    use super::Instance;

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
        chat_instance.update(update_operation.clone()).unwrap();

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
