// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;

use futures_util::{Stream, TryStreamExt, stream};
use p2panda_core::logs::{LogHeights, LogRanges, Logs};
use p2panda_core::{AnyOperation, Hash, LogId, SeqNum, VerifyingKey};
use p2panda_store::logs::LogStore;

type OperationStream<Error> =
    Pin<Box<dyn Stream<Item = Result<(AnyOperation, Vec<u8>), Error>> + Send>>;

/// Compute log heights of all passed author logs based on what is known in the local store.
pub async fn get_log_heights<L, S>(
    store: &S,
    logs: &Logs<VerifyingKey, L>,
) -> Result<LogHeights<VerifyingKey, L>, S::Error>
where
    L: LogId,
    S: LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash> + Clone + Send + 'static,
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
/// Operations are queried per-log so as to avoid holding all operations in memory at one time.
pub fn stream_log_ranges<S, L>(
    store: &S,
    ranges: LogRanges<VerifyingKey, L>,
) -> OperationStream<S::Error>
where
    S: LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash> + Clone + Send + 'static,
    L: LogId + Clone + Send + 'static,
{
    // Flatten log ranges.
    let mut flattened_ranges = vec![];
    for (author, log_heights) in ranges {
        for (log_id, (after, until)) in log_heights {
            flattened_ranges.push((author, log_id, after, until));
        }
    }

    // Query each log range in a stream iterator which produces the query result when polled.
    let store = store.clone();
    let stream = stream::try_unfold(
        (0usize, flattened_ranges, store),
        |(index, ranges, store)| async move {
            if index >= ranges.len() {
                return Ok::<_, S::Error>(None);
            }

            let (author, log_id, after, until) = &ranges[index];

            let rows = store
                .get_log_entries(author, log_id, *after, *until)
                .await?
                .unwrap_or_default();

            Ok(Some((rows, (index + 1, ranges, store))))
        },
    );

    // Flatten the query stream so that an AnyOperation is produced on each call.
    let stream = stream
        .map_ok(|rows| {
            stream::iter(
                rows.into_iter()
                    .map(|(operation, header_bytes)| Ok((operation, header_bytes))),
            )
        })
        .try_flatten();

    Box::pin(stream)
}
