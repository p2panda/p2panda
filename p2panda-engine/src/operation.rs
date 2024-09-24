// SPDX-License-Identifier: AGPL-3.0-or-later

use ciborium::de::Error as CiboriumError;
use p2panda_core::{
    validate_backlink, validate_operation, Body, Header, Operation, OperationError,
};
use p2panda_store::{LogStore, OperationStore, StoreError};
use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

/// Encoded bytes of an operation header and optional body.
pub type RawOperation = (Vec<u8>, Option<Vec<u8>>);

/// Decodes operation header and optional body represented as CBOR bytes.
///
/// Fails when payload contains invalid encoding.
pub fn decode_operation<E>(
    header: &[u8],
    body: Option<&[u8]>,
) -> Result<(Header<E>, Option<Body>), DecodeError>
where
    E: DeserializeOwned,
{
    let header =
        ciborium::from_reader::<Header<E>, _>(header).map_err(Into::<DecodeError>::into)?;
    let body = body.map(Body::new);
    Ok((header, body))
}

#[derive(Debug, Error)]
pub enum DecodeError {
    /// An error occurred while reading bytes
    ///
    /// Contains the underlying error returned while reading.
    #[error("an error occurred while reading bytes: {0}")]
    Io(std::io::Error),

    /// An error occurred while parsing bytes
    ///
    /// Contains the offset into the stream where the syntax error occurred.
    #[error("an error occurred while parsing bytes at position {0}")]
    Syntax(usize),

    /// An error occurred while processing a parsed value
    ///
    /// Contains a description of the error that occurred and (optionally) the offset into the
    /// stream indicating the start of the item being processed when the error occurred.
    #[error("an error occurred while processing a parsed value at position {0:?}: {1}")]
    Semantic(Option<usize>, String),

    /// The input caused serde to recurse too much
    ///
    /// This error prevents a stack overflow.
    #[error("recursion limit exceeded while decoding")]
    RecursionLimitExceeded,
}

impl From<CiboriumError<std::io::Error>> for DecodeError {
    fn from(value: CiboriumError<std::io::Error>) -> Self {
        match value {
            CiboriumError::Io(err) => DecodeError::Io(err),
            CiboriumError::Syntax(offset) => DecodeError::Syntax(offset),
            CiboriumError::Semantic(offset, description) => {
                DecodeError::Semantic(offset, description)
            }
            CiboriumError::RecursionLimitExceeded => DecodeError::RecursionLimitExceeded,
        }
    }
}

#[derive(Debug)]
pub enum IngestResult<E> {
    /// Operation has been successfully validated and persisted.
    Complete(Operation<E>),

    /// We're missing previous operations before we can try validating the backlink of this
    /// operation.
    ///
    /// The number indicates how many operations we are lacking before we can attempt validation
    /// again.
    Retry(Header<E>, Option<Body>, u64),
}

/// Checks an incoming operation for log integrity and persists it into the store when valid.
///
/// This method also automatically prunes the log when a prune flag was set.
///
/// If the operation seems valid but we're still lacking information (as it might have arrived
/// out-of-order) this method does not fail but indicates that we might have to retry again later.
// @TODO: Move this into `p2panda-core`
pub async fn ingest_operation<S, L, E>(
    store: &mut S,
    header: Header<E>,
    body: Option<Body>,
    log_id: &L,
    prune_flag: bool,
) -> Result<IngestResult<E>, IngestError>
where
    S: OperationStore<L, E> + LogStore<L, E>,
    E: Clone + Serialize + DeserializeOwned,
{
    let operation = Operation {
        hash: header.hash(),
        header,
        body,
    };

    if let Err(err) = validate_operation(&operation) {
        return Err(IngestError::InvalidOperation(err));
    }

    let already_exists = store.get_operation(operation.hash).await?.is_some();
    if !already_exists {
        // If no pruning flag is set, we expect the log to have integrity with the previously given
        // operation
        if !prune_flag && operation.header.seq_num > 0 {
            let latest_operation = store
                .latest_operation(&operation.header.public_key, log_id)
                .await?;

            match latest_operation {
                Some(latest_operation) => {
                    if let Err(err) = validate_backlink(&latest_operation.header, &operation.header)
                    {
                        match err {
                            // These errors signify that the sequence number is monotonic
                            // incrementing and correct, however the backlink does not match
                            OperationError::BacklinkMismatch
                            | OperationError::BacklinkMissing
                            // Log can only contain operations from one author
                            | OperationError::TooManyAuthors => {
                                return Err(IngestError::InvalidOperation(err))
                            }
                            // We observe a gap in the log and therefore can't validate the
                            // backlink yet
                            OperationError::SeqNumNonIncremental(expected, given) => {
                                return Ok(IngestResult::Retry(operation.header, operation.body, given - expected))
                            }
                            _ => unreachable!("other error cases have been handled before"),
                        }
                    }
                }
                // We're missing the whole log so far
                None => {
                    return Ok(IngestResult::Retry(
                        operation.header.clone(),
                        operation.body.clone(),
                        operation.header.seq_num,
                    ))
                }
            }
        }

        store.insert_operation(&operation, log_id).await?;

        if prune_flag {
            store
                .delete_operations(
                    &operation.header.public_key,
                    log_id,
                    operation.header.seq_num,
                )
                .await?;
        }
    }

    Ok(IngestResult::Complete(operation))
}

#[derive(Debug, Error)]
pub enum IngestError {
    /// Operation can not be authenticated, has broken log- or payload integrity or doesn't follow
    /// the p2panda specification.
    #[error("operation validation failed: {0}")]
    InvalidOperation(OperationError),

    /// Header did not contain the extensions required by the p2panda specification.
    #[error("missing \"{0}\" extension in header")]
    MissingHeaderExtension(String),

    /// Critical storage failure occurred. This is usually a reason to panic.
    #[error(transparent)]
    StoreError(#[from] StoreError),

    /// Some implementations might optimistically retry to ingest operations which arrived
    /// out-of-order. This error comes up when all given attempts have been exhausted.
    #[error("too many attempts to ingest out-of-order operation ({0} behind in log)")]
    MaxAttemptsReached(u64),
}

#[cfg(test)]
mod tests {
    use p2panda_core::{Hash, Header, PrivateKey};
    use p2panda_store::MemoryStore;

    use crate::operation::{ingest_operation, IngestResult};
    use crate::test_utils::Extensions;

    #[tokio::test]
    async fn retry_result() {
        let mut store = MemoryStore::<usize, Extensions>::new();
        let private_key = PrivateKey::new();
        let log_id = 1;

        // 1. Create a regular first operation in a log
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
            extensions: None,
        };
        header.sign(&private_key);

        let result = ingest_operation(&mut store, header, None, &log_id, false).await;
        assert!(matches!(result, Ok(IngestResult::Complete(_))));

        // 2. Create an operation which has already advanced in the log (it has a backlink and
        //    higher sequence number)
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
            extensions: None,
        };
        header.sign(&private_key);

        let result = ingest_operation(&mut store, header, None, &log_id, false).await;
        assert!(matches!(result, Ok(IngestResult::Retry(_, None, 11))));
    }
}
