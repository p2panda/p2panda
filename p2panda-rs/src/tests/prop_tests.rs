// SPDX-License-Identifier: AGPL-3.0-or-later

//! Use property testing to check that p2panda works across all possible payloads.
//!
//! These tests generate random payloads to provide another dimension of testing against edge cases
//! and combinations of the functionality provided by the library. Have a look at the proptest
//! book to learn more: <https://altsysrq.github.io/proptest-book/intro.html>

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
