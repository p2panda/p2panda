// SPDX-License-Identifier: MIT OR Apache-2.0

use futures_util::stream;
use futures_util::{StreamExt, future};
pub use p2panda_core::logs::{LogHeights, LogRanges, Logs, compare_logs};
use p2panda_core::{AnyOperation, Hash, LogId, SeqNum, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
#[cfg(feature = "ingest")]
pub use p2panda_stream::ingest::{
    IngestError, IngestResult, OooBuffer, OooResult, ingest_operation,
};
use thiserror::Error;

/// Item delivered from a [`LogStream`].
pub type LogEntry<L> = p2panda_store::logs::LogEntry<AnyOperation, L>;

/// Stream of operations in a log range.
pub type LogStream<L, E> = p2panda_store::logs::LogStream<AnyOperation, L, E>;

/// Compute log heights of all passed author logs based on what is known in the local store.
pub async fn log_heights<L, S>(
    store: &S,
    logs: &Logs<VerifyingKey, L>,
) -> Result<LogHeights<VerifyingKey, L>, S::Error>
where
    L: LogId,
    S: LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash>,
{
    let mut result = LogHeights::new();
    for (verifying_key, log_ids) in logs {
        let Some(log_heights) = store.get_log_heights(verifying_key, log_ids).await? else {
            continue;
        };
        result.insert(*verifying_key, log_heights);
    }

    Ok(result)
}

/// Construct a stream which returns all operations in the provided log ranges.
///
/// This is a memory efficient query with only one stream item staying in memory at a time. If any
/// error occurs when fetching items the stream will return the error then immediately close.
pub fn log_ranges<L, S>(store: &S, ranges: LogRanges<VerifyingKey, L>) -> LogStream<L, S::Error>
where
    L: LogId + Send + 'static,
    S: LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash> + Clone + Send + 'static,
{
    let mut flattened_ranges = vec![];
    for (author, log_heights) in ranges {
        for (log_id, (after, until)) in log_heights {
            flattened_ranges.push((author, log_id, after, until));
        }
    }

    let store = store.clone();

    let stream = stream::iter(flattened_ranges)
        // Flatten all inner streams into one.
        .flat_map(move |(author, log_id, after, until)| {
            match store.log_entries(&author, &log_id, after, until) {
                Ok(stream) => stream,
                // If an error occurs when constructing the next stream, return a new stream
                // containing one item, the error itself.
                Err(err) => Box::pin(stream::once(async move { Err(err) })),
            }
        })
        // If any error is observed then immediately close the stream.
        .scan(false, |error_occurred, item| {
            if *error_occurred {
                return future::ready(None);
            }
            *error_occurred = item.is_err();
            future::ready(Some(item))
        });

    Box::pin(stream)
}

/// Compute log heights for the given topic based on what is known in the local store.
pub async fn topic_log_heights<L, S, T>(
    store: &S,
    topic: &T,
) -> Result<LogHeights<VerifyingKey, L>, StoreError<L, S, T>>
where
    L: LogId,
    S: TopicStore<T, VerifyingKey, L> + LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash>,
{
    let logs: Logs<VerifyingKey, L> = store
        .resolve(topic)
        .await
        .map_err(|err| StoreError::TopicStore(err))?;
    let log_heights = log_heights(store, &logs)
        .await
        .map_err(|err| StoreError::LogStore(err))?;

    Ok(log_heights)
}

/// Critical store failure.
#[derive(Debug, Error)]
pub enum StoreError<L, S, T>
where
    L: LogId,
    S: TopicStore<T, VerifyingKey, L> + LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash>,
{
    #[error(transparent)]
    LogStore(<S as LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash>>::Error),

    #[error(transparent)]
    TopicStore(<S as TopicStore<T, VerifyingKey, L>>::Error),
}
