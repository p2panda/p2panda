// SPDX-License-Identifier: AGPL-3.0-or-later

use ciborium::de::Error as CiboriumError;
use p2panda_core::{
    validate_backlink, validate_operation, Body, Extension, Header, Operation, OperationError,
};
use p2panda_store::{LogStore, OperationStore, StoreError};
use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

use crate::extensions::{PruneFlag, StreamName};

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
/// This method also automatically prunes the log when a prune flag was set in the header.
///
/// If the operation seems valid but we're still lacking information (as it might have arrived
/// out-of-order) this method does not fail but indicates that we might have to retry again later.
///
/// The trait bounds requires the operation header to contain a prune flag and stream name as
/// specified by the core p2panda specification.
pub async fn ingest_operation<S, E>(
    store: &mut S,
    header: Header<E>,
    body: Option<Body>,
) -> Result<IngestResult<E>, IngestError>
where
    S: OperationStore<StreamName, E> + LogStore<StreamName, E>,
    E: Clone + Serialize + DeserializeOwned + Extension<StreamName> + Extension<PruneFlag>,
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
        let stream_name: StreamName = operation
            .header
            .extract()
            .ok_or(IngestError::MissingHeaderExtension("stream_name".into()))?;
        let prune_flag: PruneFlag = operation
            .header
            .extract()
            .ok_or(IngestError::MissingHeaderExtension("prune_flag".into()))?;

        // If no pruning flag is set, we expect the log to have integrity with the previously given
        // operation
        // @TODO: Move this into `p2panda-core`
        if !prune_flag.is_set() && operation.header.seq_num > 0 {
            let latest_operation = store
                .latest_operation(&operation.header.public_key, &stream_name)
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

        store.insert_operation(&operation, &stream_name).await?;

        if prune_flag.is_set() {
            store
                .delete_operations(
                    &operation.header.public_key,
                    &stream_name,
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
}
