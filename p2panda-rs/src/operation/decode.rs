// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use crate::operation::{Operation, OperationError, RawOperation};
use crate::schema::Schema;

pub fn decode_operation(bytes: &[u8], schema: &Schema) -> Result<Operation, OperationError> {
    let raw_operation = decode_raw_operation(&bytes)?;
    let operation = Operation::from_raw(&raw_operation, schema)?;
    Ok(operation)
}

fn decode_raw_operation(bytes: &[u8]) -> Result<RawOperation, OperationError> {
    let raw_operation: RawOperation =
        ciborium::de::from_reader(bytes).map_err(|_| OperationError::EmptyPreviousOperations)?;
    Ok(raw_operation)
}
