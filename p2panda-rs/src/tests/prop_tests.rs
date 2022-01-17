use std::convert::TryFrom;

// Bring the macros and other important things into scope.
use crate::hash::Hash;
use crate::operation::{Operation, OperationEncoded, OperationFields};
use proptest::prelude::*;

proptest! {
    #[test]
    fn operation_value_as_parameter(fields: OperationFields, hash: Hash) {
        let operation = Operation::new_create(hash, fields.clone());

        if fields.is_empty() {
            prop_assert!(operation.is_err());
        } else {
            prop_assert!(operation.is_ok());

            let operation_encoded = OperationEncoded::try_from(operation.as_ref().unwrap()).unwrap();
            let decoded_operation = Operation::from(&operation_encoded);

            prop_assert_eq!(decoded_operation, operation.unwrap());
        }
    }
}
