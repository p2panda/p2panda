// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::operation::{Operation, OperationError, RawOperation};

pub fn encode_operation(operation: &Operation) -> Result<Vec<u8>, OperationError> {
    // Convert to raw operation format
    let raw_operation: RawOperation = operation.into();

    // Encode as CBOR byte sequence
    let mut cbor_bytes = Vec::new();
    ciborium::ser::into_writer(&raw_operation, &mut cbor_bytes)
        .map_err(|_| OperationError::EmptyPreviousOperations)?; // @TODO: Correct error type

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
