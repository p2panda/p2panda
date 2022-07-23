// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::operation::{verify_schema_and_convert, Operation, RawOperation, RawOperationError};
use crate::schema::Schema;
use crate::Validate;

pub fn decode_operation(bytes: &[u8], schema: &Schema) -> Result<Operation, RawOperationError> {
    let raw_operation = decode_raw_operation(&bytes)?;
    let operation = verify_schema_and_convert(&raw_operation, schema)?;
    Ok(operation)
}

fn decode_raw_operation(bytes: &[u8]) -> Result<RawOperation, RawOperationError> {
    let raw_operation: RawOperation = ciborium::de::from_reader(bytes)
        .map_err(|err| RawOperationError::InvalidCBOREncoding(err.to_string()))?;

    // @TODO: Do we need this?!
    raw_operation.validate()?;

    Ok(raw_operation)
}
