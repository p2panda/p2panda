// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use proptest::prelude::*;

use crate::operation::{Operation, OperationEncoded, OperationFields};
use crate::test_utils::fixtures::defaults::schema;

proptest! {
    #[test]
    fn operation_value_as_parameter(fields: OperationFields) {
        // Create an operation using the default testing schema hash
        let operation = Operation::new_create(schema(), fields.clone());

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
