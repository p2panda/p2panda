// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods to encode operations.
//!
//! Encoding an operation does not require a schema, the `Operation` will be serialized into bytes,
//! represented as a `EncodedOperation` which is then ready to be sent to a p2panda node (alongside
//! an entry).
//!
//! ```text
//! ┌─────────┐                           ┌────────────────┐
//! │Operation│ ───encode_operation()───► │EncodedOperation│ ────► bytes
//! └─────────┘                           └────────────────┘
//! ```
use crate::operation::error::EncodeOperationError;
use crate::operation::plain::PlainOperation;
use crate::operation::{EncodedOperation, Operation};

/// Encodes an operation in canonic format.
pub fn encode_operation(operation: &Operation) -> Result<EncodedOperation, EncodeOperationError> {
    // Convert to plain operation format
    let plain: PlainOperation = operation.into();

    // Encode as CBOR byte sequence
    let encoded_operation = encode_plain_operation(&plain)?;

    Ok(encoded_operation)
}

/// Encodes a `PlainOperation` instance in canonic format.
pub fn encode_plain_operation(
    plain: &PlainOperation,
) -> Result<EncodedOperation, EncodeOperationError> {
    let mut cbor_bytes = Vec::new();

    ciborium::ser::into_writer(&plain, &mut cbor_bytes).map_err(|err| match err {
        ciborium::ser::Error::Io(err) => EncodeOperationError::EncoderIOFailed(err.to_string()),
        ciborium::ser::Error::Value(err) => EncodeOperationError::EncoderFailed(err),
    })?;

    Ok(EncodedOperation::from_bytes(&cbor_bytes))
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use rstest::rstest;

    use crate::operation::plain::PlainOperation;
    use crate::operation::Operation;
    use crate::serde::encode_value;
    use crate::test_utils::fixtures::operation_with_schema;

    use super::{encode_operation, encode_plain_operation};

    #[rstest]
    fn encoding(#[from(operation_with_schema)] operation: Operation) {
        let plain_operation = PlainOperation::from(&operation);

        // Test both methods to encode operations and compare
        let from_operation = encode_operation(&operation).unwrap();
        let from_plain_operation = encode_plain_operation(&plain_operation).unwrap();

        assert_eq!(from_operation.to_string(), from_plain_operation.to_string());
        assert_eq!(
            encode_value(cbor!(
                [
                    1,
                    0,
                    "venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b",
                    {
                        "age" => 28,
                        "comments" => [
                            [
                                "00204f0dd3a1b8205b6d4ce3fd4c158bb91c9e131bd842e727164ea220b5b6d09346"
                            ],
                            [
                                "002019ed3e9b39cd17f1dbc0f6e31a6e7b9c9ab7e349332e710c946a441b7d308eb5",
                                "0020995d53f460293c5686c42037b72787ed28668ad8b6d18e9d5f02c5d3301161f0"
                            ]
                        ],
                        "height" => 3.5,
                        "is_admin" => false,
                        "my_friends" => [
                            "00209a2149589672fa1ac2348e48b4c56fc208a0eff44938464dd2091850f444a323"
                        ],
                        "past_event" => [
                            "0020475488c0e2bbb9f5a81929e2fe11de81c1f83c8045de41da43899d25ad0d4afa",
                            "0020f7a17e14b9a5e87435decdbc28d562662fbf37da39b94e8469d8e1873336e80e"
                        ],
                        "profile_picture" => "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543",
                        "username" => "bubu"
                    }
            ])),
            from_operation.into_bytes()
        );
    }
}
