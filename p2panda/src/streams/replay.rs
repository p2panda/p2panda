// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::logs::LogRanges;
use p2panda_core::{Cursor, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::{SqliteError, SqliteStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::debug;

use crate::operation::{Extensions, LogId, Operation};
use crate::processor::Pipeline;
use crate::streams::StreamEvent;
use crate::streams::stream::{Source, process_operation_in};

/// Determines the starting point of a subscription stream.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub enum StreamFrom {
    /// Stream all events from the beginning, including already acknowledged ones.
    ///
    /// `Start` is useful if the application doesn't keep any materialised state around and needs to
    /// repeat all messages from the beginning. Another use-case is the roll-out of an application
    /// update where all state needs to be re-materialised.
    Start,

    /// Stream only unacknowledged events from where we've ended last.
    ///
    /// We keep an internal cursor around for each topic which is used to track acknowledged
    /// operations.
    #[default]
    Frontier,

    /// Stream all events from _after_ the given cursor position.
    Cursor(Cursor<VerifyingKey, LogId>),
}

impl From<Cursor<VerifyingKey, LogId>> for StreamFrom {
    fn from(cursor: Cursor<VerifyingKey, LogId>) -> Self {
        Self::Cursor(cursor)
    }
}

/// Re-play and re-process locally stored operations.
pub(crate) async fn replay_log_ranges<M>(
    topic: Topic,
    store: &SqliteStore,
    to_output_tx: &mpsc::Sender<StreamEvent<M>>,
    pipeline: &Pipeline<LogId, Extensions, Topic>,
    log_ranges: LogRanges<VerifyingKey, LogId>,
) -> Result<(), ReplayError>
where
    M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    let total_operations = total_operations(&log_ranges);
    debug!("replay {total_operations} operations");

    if total_operations == 0 {
        return Ok(());
    }

    to_output_tx
        .send(StreamEvent::ReplayStarted { total_operations })
        .await
        .map_err(|_| ReplayError::CriticalError)?;

    for (author, logs) in log_ranges {
        for (log_id, (after, until)) in logs {
            let Some(operations): Option<Vec<(Operation, _)>> = store
                .get_log_entries(&author, &log_id, after, until)
                .await?
            else {
                // If the log was concurrently deleted since calling TopicStore::resolve then None
                // is returned here. This is not considered an error, as no log integrity is broken
                // and deletes should be immediately respected.
                continue;
            };

            for (operation, _) in operations {
                process_operation_in(operation, Source::LocalStore, topic, pipeline).await;
            }
        }
    }

    to_output_tx
        .send(StreamEvent::ReplayEnded)
        .await
        .map_err(|_| ReplayError::CriticalError)?;

    debug!("finished replaying {total_operations} operations");

    Ok(())
}

/// Returns the number of all operations in the given log ranges.
fn total_operations<A, L>(log_ranges: &LogRanges<A, L>) -> u32 {
    log_ranges.iter().fold(0, |mut acc, (_, logs)| {
        logs.iter().for_each(|(_, (after, until))| {
            // Ignore open-ended ranges.
            if let Some(until) = until {
                if after.is_none() {
                    // First operation seq_num = 0 counts as an item.
                    //
                    // TODO: Range semantics might be a bit confusing here. See related issue:
                    // https://github.com/p2panda/p2panda/issues/1184
                    acc += 1;
                }

                // Add all other operations following.
                let after = after.unwrap_or_default();
                acc += until - after
            }
        });

        acc
    })
}

/// Topic stream could not re-play events due to an internal error.
#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("an error occurred while querying the store: {0}")]
    Store(#[from] SqliteError),

    #[error("a critical error occurred in the replay task")]
    CriticalError,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use p2panda_core::logs::LogRanges;
    use p2panda_core::{SigningKey, VerifyingKey};

    use super::total_operations;

    #[test]
    fn calculate_total_operations() {
        let author_1 = SigningKey::generate().verifying_key();
        let author_2 = SigningKey::generate().verifying_key();
        let author_3 = SigningKey::generate().verifying_key();

        let ranges_1 = {
            let mut result: LogRanges<VerifyingKey, usize> = BTreeMap::new();
            let logs = result.entry(author_1).or_default();
            // [0 1 2 3 4 5 6 7 8 9 10 11 12] = 13
            logs.insert(0, (None, Some(12)));
            result
        };
        assert_eq!(total_operations(&ranges_1), 13);

        let ranges_2 = {
            let mut result: LogRanges<VerifyingKey, usize> = BTreeMap::new();
            let logs = result.entry(author_1).or_default();
            // [1 2 3 4 5 6 7 8 9 10 11 12] = 12
            logs.insert(0, (Some(0), Some(12)));
            // [6 7] = 2
            logs.insert(1, (Some(5), Some(7)));
            // [78 79 80] = 3
            logs.insert(2, (Some(77), Some(80)));
            let logs = result.entry(author_2).or_default();
            // [0 1 2 3] = 4
            logs.insert(3, (None, Some(3)));
            let logs = result.entry(author_3).or_default();
            // [11 12 13 14 15 16 17 18 19 20] = 10
            logs.insert(4, (Some(10), Some(20)));
            result
        };
        assert_eq!(total_operations(&ranges_2), 12 + 2 + 3 + 4 + 10);

        let ranges_3 = {
            let mut result: LogRanges<VerifyingKey, usize> = BTreeMap::new();
            let logs = result.entry(author_1).or_default();
            // Ignore this one, since it is an open-ended (invalid) range.
            logs.insert(0, (Some(100), None));
            result
        };
        assert_eq!(total_operations(&ranges_3), 0);

        let ranges_4 = {
            let mut result: LogRanges<VerifyingKey, usize> = BTreeMap::new();
            let logs = result.entry(author_1).or_default();
            // [ 0 ] = 1
            logs.insert(0, (None, Some(0)));
            result
        };
        assert_eq!(total_operations(&ranges_4), 1);

        let ranges_5 = {
            let mut result: LogRanges<VerifyingKey, usize> = BTreeMap::new();
            let logs = result.entry(author_1).or_default();
            // [] = 0
            logs.insert(0, (Some(0), Some(0)));
            result
        };
        assert_eq!(total_operations(&ranges_5), 0);

        let ranges_6 = {
            let mut result: LogRanges<VerifyingKey, usize> = BTreeMap::new();
            let logs = result.entry(author_1).or_default();
            // No range given.
            logs.insert(0, (None, None));
            result
        };
        assert_eq!(total_operations(&ranges_6), 0);

        let ranges_7 = {
            let mut result: LogRanges<VerifyingKey, usize> = BTreeMap::new();
            let logs = result.entry(author_1).or_default();
            // [] = 0
            logs.insert(0, (Some(1), Some(1)));
            result
        };
        assert_eq!(total_operations(&ranges_7), 0);
    }
}
