// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::operation::error::EncodeOperationError;
use crate::operation::plain::PlainOperation;
use crate::operation::Operation;

pub fn encode_operation(operation: &Operation) -> Result<Vec<u8>, EncodeOperationError> {
    // Convert to plain operation format
    let plain: PlainOperation = operation.into();

    // Encode as CBOR byte sequence
    let cbor_bytes = encode_plain_operation(&plain)?;

    Ok(cbor_bytes)
}

pub fn encode_plain_operation(plain: &PlainOperation) -> Result<Vec<u8>, EncodeOperationError> {
    let mut cbor_bytes = Vec::new();

    ciborium::ser::into_writer(&plain, &mut cbor_bytes).map_err(|err| match err {
        ciborium::ser::Error::Io(err) => EncodeOperationError::EncoderIOFailed(err.to_string()),
        ciborium::ser::Error::Value(err) => EncodeOperationError::EncoderFailed(err.to_string()),
    })?;

    Ok(cbor_bytes)
}

#[cfg(test)]
mod tests {
    use crate::operation::{Operation, OperationFields, OperationValue};
    use crate::schema::SchemaId;

    use super::encode_operation;

    #[test]
    fn encode() {
        let mut fields = OperationFields::new();
        fields
            .add("name", OperationValue::Text("venue".to_owned()))
            .unwrap();
        fields
            .add("type", OperationValue::Text("str".to_owned()))
            .unwrap();

        let operation = Operation::new_create(SchemaId::SchemaFieldDefinition(1), fields).unwrap();
        let bytes = encode_operation(&operation).unwrap();

        assert_eq!(
            bytes,
            vec![
                132, 1, 0, 120, 26, 115, 99, 104, 101, 109, 97, 95, 102, 105, 101, 108, 100, 95,
                100, 101, 102, 105, 110, 105, 116, 105, 111, 110, 95, 118, 49, 162, 100, 110, 97,
                109, 101, 101, 118, 101, 110, 117, 101, 100, 116, 121, 112, 101, 99, 115, 116, 114
            ]
        );
    }
}
