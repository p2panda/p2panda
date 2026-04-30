// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::logs::LogRanges;
use p2panda_core::{Cursor, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::{SqliteError, SqliteStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;

use crate::node::AckPolicy;
use crate::operation::{Extensions, LogId, Operation};
use crate::processor::Pipeline;
use crate::streams::StreamEvent;
use crate::streams::acked::Acked;
use crate::streams::stream::{Source, process_operation};

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
    app_tx: &mpsc::Sender<StreamEvent<M>>,
    pipeline: &Pipeline<LogId, Extensions, Topic>,
    ack_policy: AckPolicy,
    acked: &Acked,
    log_ranges: LogRanges<VerifyingKey, LogId>,
) -> Result<(), ReplayError>
where
    M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    for (author, logs) in log_ranges {
        for (log_id, (from, to)) in logs {
            let Some(operations): Option<Vec<(Operation, _)>> =
                store.get_log_entries(&author, &log_id, from, to).await?
            else {
                // If the log was concurrently deleted since calling TopicStore::resolve then None
                // is returned here. This is not considered an error, as no log integrity is broken
                // and deletes should be immediately respected.
                continue;
            };

            for (operation, _) in operations {
                match process_operation::<M>(
                    operation,
                    topic,
                    pipeline,
                    ack_policy,
                    acked,
                    Source::LocalStore,
                )
                .await
                {
                    Some(event) => {
                        app_tx
                            .send(event)
                            .await
                            .map_err(|_| ReplayError::CriticalError)?;
                    }
                    None => continue,
                }
            }
        }
    }

    Ok(())
}

/// Topic stream could not re-play events due to an internal error.
#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("an error occurred while querying the store: {0}")]
    Store(#[from] SqliteError),

    #[error("a critical error occurred in the replay task")]
    CriticalError,
}
