// SPDX-License-Identifier: AGPL-3.0-or-later

//! Types and methods for deriving and maintaining `Instances`.

use std::{collections::HashMap, convert::TryFrom};

use crate::operation::{Operation, OperationValue};

use super::error::InstanceError;

/// The materialised view of a reduced collection of `Operations`
pub type Instance = HashMap<String, OperationValue>;

impl TryFrom<Operation> for Instance {
    type Error = InstanceError;

    fn try_from(operation: Operation) -> Result<Instance, InstanceError> {
        if !operation.is_create() {
            return Err(InstanceError::NotCreateOperation);
        };

        let mut instance = Instance::new();
        let fields = operation.fields();

        if let Some(fields) = fields {
            for (key, value) in fields.iter() {
                instance.insert(key.to_string(), value.to_owned());
            }
        }

        Ok(instance)
    }
}

#[cfg(test)]
mod tests {
    use std::convert::{TryFrom, TryInto};

    use rstest::rstest;

    use super::Instance;
    use crate::operation::{Operation, OperationValue};
    use crate::test_utils::fixtures::{create_operation, delete_operation, update_operation};

    #[rstest]
    fn try_from_operation(
        create_operation: Operation,
        update_operation: Operation,
        delete_operation: Operation,
    ) {
        // Convert a CREATE `Operation` into an `Instance`
        let instance: Instance = create_operation.try_into().unwrap();

        let mut expected_instance = Instance::new();
        expected_instance.insert(
            "message".to_string(),
            OperationValue::Text("Hello!".to_string()),
        );

        assert_eq!(instance, expected_instance);

        // Convert an UPDATE or DELETE `Operation` into an `Instance`
        let instance_1 = Instance::try_from(update_operation);
        let instance_2 = Instance::try_from(delete_operation);

        assert!(instance_1.is_err());
        assert!(instance_2.is_err());
    }
}
