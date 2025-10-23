// SPDX-License-Identifier: MIT OR Apache-2.0

//! Methods to handle p2panda operations.
use p2panda_core::{
    Body, Extensions, Header, Operation, OperationError, validate_backlink, validate_operation,
};
use p2panda_store::{LogStore, OperationStore};
use thiserror::Error;

/// Checks an incoming operation for log integrity and persists it into the store when valid.
///
/// This method also automatically prunes the log when a prune flag was set.
///
/// If the operation seems valid but we're still lacking information (as it might have arrived
/// out-of-order) this method does not fail but indicates that we might have to retry again later.
pub async fn ingest_operation<S, L, E>(
    store: &mut S,
    header: Header<E>,
    body: Option<Body>,
    header_bytes: Vec<u8>,
    log_id: &L,
    prune_flag: bool,
) -> Result<IngestResult<E>, IngestError>
where
    S: OperationStore<L, E> + LogStore<L, E>,
    E: Extensions,
{
    let operation = Operation {
        hash: header.hash(),
        header,
        body,
    };

    if let Err(err) = validate_operation(&operation) {
        return Err(IngestError::InvalidOperation(err));
    }

    let already_exists = store
        .has_operation(operation.hash)
        .await
        .map_err(|err| IngestError::StoreError(err.to_string()))?;
    if !already_exists {
        // If no pruning flag is set, we expect the log to have integrity with the previously given
        // operation.
        if !prune_flag && operation.header.seq_num > 0 {
            let latest_operation = store
                .latest_operation(&operation.header.public_key, log_id)
                .await
                .map_err(|err| IngestError::StoreError(err.to_string()))?;

            match latest_operation {
                Some(latest_operation) => {
                    if let Err(err) = validate_backlink(&latest_operation.0, &operation.header) {
                        match err {
                            // These errors signify that the sequence number is monotonic
                            // incrementing and correct, however the backlink does not match.
                            OperationError::BacklinkMismatch
                            | OperationError::BacklinkMissing
                            // Log can only contain operations from one author.
                            | OperationError::TooManyAuthors => {
                                return Err(IngestError::InvalidOperation(err))
                            }
                            // We observe a gap in the log and therefore can't validate the
                            // backlink yet.
                            OperationError::SeqNumNonIncremental(expected, given) => {
                                return Ok(IngestResult::Retry(operation.header, operation.body, header_bytes, given - expected))
                            }
                            _ => unreachable!("other error cases have been handled before"),
                        }
                    }

                    let mut missing = 0;
                    for previous in &operation.header.previous {
                        if !store.has_operation(*previous).await.map_err(|err| {
                            IngestError::StoreError(format!(
                                "could not look up previous operation: {}",
                                err
                            ))
                        })? {
                            missing += 1;
                        }
                    }
                    if missing > 0 {
                        return Ok(IngestResult::Retry(
                            operation.header,
                            operation.body,
                            header_bytes,
                            missing,
                        ));
                    }
                }
                // We're missing the whole log so far.
                None => {
                    return Ok(IngestResult::Retry(
                        operation.header.clone(),
                        operation.body.clone(),
                        header_bytes,
                        operation.header.seq_num,
                    ));
                }
            }
        }

        store
            .insert_operation(
                operation.hash,
                &operation.header,
                operation.body.as_ref(),
                &header_bytes,
                log_id,
            )
            .await
            .map_err(|err| IngestError::StoreError(err.to_string()))?;

        if prune_flag {
            store
                .delete_operations(
                    &operation.header.public_key,
                    log_id,
                    operation.header.seq_num,
                )
                .await
                .map_err(|err| IngestError::StoreError(err.to_string()))?;
        }
    }

    Ok(IngestResult::Complete(operation))
}

/// Operations can be ingested directly or need to be re-tried if they arrived out-of-order.
#[derive(Debug)]
pub enum IngestResult<E> {
    /// Operation has been successfully validated and persisted.
    Complete(Operation<E>),

    /// We're missing previous operations before we can try validating the backlink of this
    /// operation.
    ///
    /// The number indicates how many operations we are lacking before we can attempt validation
    /// again.
    Retry(Header<E>, Option<Body>, Vec<u8>, u64),
}

/// Errors which can occur due to invalid operations or critical storage failures.
#[derive(Clone, Debug, Error)]
pub enum IngestError {
    /// Operation can not be authenticated, has broken log- or payload integrity or doesn't follow
    /// the p2panda specification.
    #[error("operation validation failed: {0}")]
    InvalidOperation(OperationError),

    /// Header did not contain the extensions required by the p2panda specification.
    #[error("missing \"{0}\" extension in header")]
    MissingHeaderExtension(String),

    /// Critical storage failure occurred. This is usually a reason to panic.
    #[error("critical storage failure: {0}")]
    StoreError(String),

    /// Some implementations might optimistically retry to ingest operations which arrived
    /// out-of-order. This error comes up when all given attempts have been exhausted.
    #[error("too many attempts to ingest out-of-order operation ({0} behind in log)")]
    MaxAttemptsReached(u64),
}

#[cfg(test)]
mod tests {
    use p2panda_core::{Hash, Header, PrivateKey};
    use p2panda_store::MemoryStore;

    use crate::operation::{IngestResult, ingest_operation};
    use crate::test_utils::Extensions;

    #[tokio::test]
    async fn retry_result() {
        let mut store = MemoryStore::<usize, Extensions>::new();
        let private_key = PrivateKey::new();
        let log_id = 1;

        // 1. Create a regular first operation in a log.
        let mut header = Header {
            public_key: private_key.public_key(),
            version: 1,
            signature: None,
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![],
            extensions: Extensions::default(),
        };
        header.sign(&private_key);
        let header_bytes = header.to_bytes();
        let hash1 = header.hash();

        let result = ingest_operation(&mut store, header, None, header_bytes, &log_id, false).await;
        assert!(matches!(result, Ok(IngestResult::Complete(_))));

        // 2. Create an operation which has already advanced in the log (it has a backlink and
        //    higher sequence number).
        let mut header = Header {
            public_key: private_key.public_key(),
            version: 1,
            signature: None,
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 12, // we'll be missing 11 operations between the first and this one
            backlink: Some(Hash::new(b"mock operation")),
            previous: vec![],
            extensions: Extensions::default(),
        };
        header.sign(&private_key);
        let header_bytes = header.to_bytes();

        let result = ingest_operation(&mut store, header, None, header_bytes, &log_id, false).await;
        assert!(matches!(result, Ok(IngestResult::Retry(_, None, _, 11))));

        // 3. Create an operation which has already advanced in the log (it has a backlink and
        //    higher sequence number).
        let mut header = Header {
            public_key: private_key.public_key(),
            version: 1,
            signature: None,
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 1,
            backlink: Some(hash1),
            previous: vec![hash1, Hash::new(b"mock operation")],
            extensions: Extensions::default(),
        };
        header.sign(&private_key);
        let header_bytes = header.to_bytes();

        let result = ingest_operation(&mut store, header, None, header_bytes, &log_id, false).await;
        assert!(matches!(result, Ok(IngestResult::Retry(_, None, _, 1))));
    }
}
