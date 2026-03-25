// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;

use p2panda_core::{Cursor, PublicKey, Topic};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore};
use p2panda_sync::protocols::{LogSyncError, get_log_heights};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::node::AckPolicy;
use crate::operation::{Extensions, LogId, Operation};
use crate::processor::Pipeline;
use crate::streams::StreamEvent;
use crate::streams::stream::{Source, process_operation};

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub enum StreamFrom {
    /// Stream all events from the beginning, including already acknowledged ones.
    Start,

    /// Stream only unacknowledged events from where we've ended last.
    ///
    /// We keep an internal cursor around for each topic which is used to track acknowledged
    /// operations.
    #[default]
    Frontier,

    /// Stream all events from _after_ the given cursor position.
    Cursor(Cursor<PublicKey, LogId>),
}

/// Compute difference between local state and given state vector for a given topic, then re-play
/// and re-process all operations in the difference set.
pub(crate) async fn replay_from<M>(
    topic: Topic,
    store: SqliteStore,
    app_tx: mpsc::Sender<StreamEvent<M>>,
    pipeline: Pipeline<LogId, Extensions, Topic>,
    ack_policy: AckPolicy,
    cursor: Cursor<PublicKey, LogId>,
) -> Result<(), ReplayError>
where
    M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    let (replay_tx, mut replay_rx) = mpsc::unbounded_channel::<Operation>();

    let replay_task: JoinHandle<Result<(), ReplayError>> = tokio::spawn(async move {
        let local_logs: BTreeMap<PublicKey, Vec<LogId>> = store.resolve(&topic).await?;

        let local_log_heights =
            get_log_heights::<LogId, Extensions, SqliteStore>(&store, &local_logs).await?;

        let diff = cursor.compare(&local_log_heights);

        for (author, logs) in diff {
            for (log_id, (from, to)) in logs {
                let Some(operations): Option<Vec<(Operation, _)>> =
                    store.get_log_entries(&author, &log_id, from, to).await?
                else {
                    // If the log was concurrently deleted since calling TopicStore::resolve then
                    // None is returned here. This is not considered an error, as no log integrity
                    // is broken and deletes should be immediately respected.
                    continue;
                };

                for (operation, _) in operations {
                    replay_tx
                        .send(operation)
                        .map_err(|_| ReplayError::CriticalError)?;
                }
            }
        }

        Ok(())
    });

    // Pull operations from the replay channel and send them to the processing pipeline.
    loop {
        if let Some(operation) = replay_rx.recv().await {
            match process_operation::<M>(
                operation,
                topic,
                &pipeline,
                ack_policy,
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
        };

        if replay_task.is_finished() {
            return replay_task
                .await
                .expect("replay task should never panic")
                .map_err(|_| ReplayError::CriticalError);
        }
    }
}

/// Error types which can occur during replay.
#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("an error occurred while querying the store: {0}")]
    Store(#[from] SqliteError),

    #[error(transparent)]
    LogSync(#[from] LogSyncError),

    #[error("a critical error occurred in the replay task")]
    CriticalError,
}
