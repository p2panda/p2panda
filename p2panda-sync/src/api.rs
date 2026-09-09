// SPDX-License-Identifier: MIT OR Apache-2.0

use futures_util::stream::{self, BoxStream};
use futures_util::{StreamExt, future};
use p2panda_core::logs::{LogHeights, LogRanges, Logs};
use p2panda_core::{AnyOperation, Hash, LogId, SeqNum, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use thiserror::Error;

pub use p2panda_store::logs::StreamItem;

/// Stream of `(AnyOperation, LogId, HeaderBytes)`.
pub type OperationStream<L, E> = BoxStream<'static, Result<StreamItem<AnyOperation, L>, E>>;

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
pub fn log_ranges<S, L>(
    store: &S,
    ranges: LogRanges<VerifyingKey, L>,
) -> OperationStream<L, S::Error>
where
    S: LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash> + Clone + Send + 'static,
    L: LogId + Send + 'static,
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
) -> Result<LogHeights<VerifyingKey, L>, TopicLogHeightsError<L, S, T>>
where
    L: LogId,
    S: TopicStore<T, VerifyingKey, L> + LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash>,
{
    let logs: Logs<VerifyingKey, L> = store
        .resolve(topic)
        .await
        .map_err(|err| TopicLogHeightsError::TopicStore(err))?;
    let log_heights = log_heights(store, &logs)
        .await
        .map_err(|err| TopicLogHeightsError::LogStore(err))?;

    Ok(log_heights)
}

#[derive(Debug, Error)]
pub enum TopicLogHeightsError<L, S, T>
where
    L: LogId,
    S: TopicStore<T, VerifyingKey, L> + LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash>,
{
    #[error(transparent)]
    LogStore(<S as LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash>>::Error),

    #[error(transparent)]
    TopicStore(<S as TopicStore<T, VerifyingKey, L>>::Error),
}
