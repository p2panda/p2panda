// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;

use p2panda_core::{PublicKey, Topic};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;

use crate::node::AckPolicy;
use crate::operation::{Extensions, Operation};
use crate::processor::Pipeline;
use crate::streams::StreamEvent;
use crate::streams::stream::process_operation;

/// Retrieve from the store and re-process all operations for a given topic.
pub(crate) async fn replay_from_start<M>(
    topic: Topic,
    store: SqliteStore,
    app_tx: Sender<StreamEvent<M>>,
    pipeline: Pipeline<Topic, Extensions, Topic>,
    ack_policy: AckPolicy,
) -> Result<(), ReplayError>
where
    M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    let (replay_tx, mut replay_rx) = mpsc::unbounded_channel::<Operation>();

    // Spawn task for retrieving operations from the store and sending them on a channel to be
    // processed.
    let replay_task: JoinHandle<Result<(), ReplayError>> = tokio::spawn(async move {
        let author_logs: BTreeMap<PublicKey, Vec<Topic>> = store.resolve(&topic).await?;
        for (author, logs) in author_logs {
            for log_id in logs {
                let Some(operations): Option<Vec<(Operation, _)>> =
                    store.get_log_entries(&author, &log_id, None, None).await?
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
            match process_operation::<M>(operation, topic, &pipeline, ack_policy).await {
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
                .expect("panic in task")
                .map_err(|_| ReplayError::CriticalError);
        }
    }
}

/// Error types which can occur during replay.
#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("an error occurred while querying the store: {0}")]
    Store(#[from] SqliteError),

    #[error("a critical error occurred in the replay task")]
    CriticalError,
}
