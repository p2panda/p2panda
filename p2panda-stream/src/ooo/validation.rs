// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{
    validate_backlink, validate_operation, Body, Extensions, Header, Operation, OperationError,
};
use p2panda_store::LogStore;
use thiserror::Error;

pub async fn validate_operation_retry<S, L, E>(
    store: &mut S,
    header: &Header<E>,
    body: Option<&Body>,
    header_bytes: &[u8],
    log_id: &L,
    prune_flag: bool,
) -> Result<ValidationResult<E>, ValidationError>
where
    S: LogStore<L, E>,
    E: Extensions,
{
    let operation = Operation {
        hash: header.hash(),
        header: header.clone(),
        body: body.cloned(),
    };

    if let Err(err) = validate_operation(&operation) {
        return Err(ValidationError::InvalidOperation(err));
    }

    // If no pruning flag is set, we expect the log to have integrity with the previously given
    // operation.
    if !prune_flag && operation.header.seq_num > 0 {
        let latest_operation = store
            .latest_operation(&operation.header.public_key, log_id)
            .await
            .map_err(|err| ValidationError::StoreError(err.to_string()))?;

        match latest_operation {
            Some(latest_operation) => {
                if let Err(err) = validate_backlink(&latest_operation.0, &operation.header) {
                    match err {
                        // These errors signify that the sequence number is monotonic incrementing
                        // and correct, however the backlink does not match.
                        OperationError::BacklinkMismatch
                        | OperationError::BacklinkMissing
                        // Log can only contain operations from one author.
                        | OperationError::TooManyAuthors => {
                            return Err(ValidationError::InvalidOperation(err))
                        }
                        // We observe a gap in the log and therefore can't validate the backlink
                        // yet.
                        OperationError::SeqNumNonIncremental(expected, given) => {
                            return Ok(ValidationResult::Retry(operation.header, operation.body, header_bytes.to_vec(), given - expected))
                        }
                        _ => unreachable!("other error cases have been handled before"),
                    }
                }
            }
            // We're missing the whole log so far.
            None => {
                let seq_num_behind = operation.header.seq_num;
                return Ok(ValidationResult::Retry(
                    operation.header,
                    operation.body,
                    header_bytes.to_vec(),
                    seq_num_behind,
                ));
            }
        }
    }

    Ok(ValidationResult::Valid(operation))
}

/// Operations can be validated directly or need to be re-tried if they arrived out-of-order.
#[derive(Debug)]
pub enum ValidationResult<E> {
    /// Operation has been successfully validated.
    Valid(Operation<E>),

    /// We're missing previous operations before we can try validating the backlink of this
    /// operation.
    ///
    /// The number indicates how many operations we are lacking before we can attempt validation
    /// again.
    Retry(Header<E>, Option<Body>, Vec<u8>, u64),
}

/// Errors which can occur due to invalid operations or critical storage failure.
#[derive(Clone, Debug, Error)]
pub enum ValidationError {
    /// Operation can not be authenticated, has broken log- or payload integrity or doesn't follow
    /// the p2panda specification.
    #[error("operation validation failed: {0}")]
    InvalidOperation(OperationError),

    /// Critical storage failure occurred. This is usually a reason to panic.
    #[error("critical storage failure: {0}")]
    StoreError(String),
}
