// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::operation::{verify_schema_and_convert, Operation, RawOperation, RawOperationError};
use crate::schema::Schema;

pub fn decode_operation(bytes: &[u8], schema: &Schema) -> Result<Operation, RawOperationError> {
    let raw_operation: RawOperation = ciborium::de::from_reader(bytes)
        .map_err(|err| RawOperationError::InvalidCBOREncoding(err.to_string()))?;

    let operation = verify_schema_and_convert(&raw_operation, schema)?;

    Ok(operation)
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use ciborium::value::Value;
    use rstest::rstest;

    use crate::operation::OperationEncoded;
    use crate::schema::Schema;
    use crate::test_utils::fixtures::{operation_encoded, schema};

    use super::decode_operation;

    fn to_cbor(value: Value) -> Vec<u8> {
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&value, &mut cbor_bytes).unwrap();
        cbor_bytes
    }
}
