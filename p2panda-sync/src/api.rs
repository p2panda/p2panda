// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;

use futures_util::{Stream, TryStreamExt, stream};
use p2panda_core::logs::LogRanges;
use p2panda_core::{AnyOperation, Hash, LogId, SeqNum, VerifyingKey};
use p2panda_store::logs::LogStore;

type OperationStream<Error> =
    Pin<Box<dyn Stream<Item = Result<(AnyOperation, Vec<u8>), Error>> + Send>>;

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
