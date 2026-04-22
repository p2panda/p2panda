// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::sync::Arc;

use p2panda_core::logs::{LogHeights, LogRanges};
use p2panda_core::{Cursor, Hash, PublicKey, SeqNum, Topic};
use p2panda_store::cursors::CursorStore;
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore, tx};
use thiserror::Error;
use tokio::sync::Semaphore;

use crate::operation::{Header, LogId, Operation};
use crate::streams::StreamFrom;

pub type Logs = BTreeMap<PublicKey, Vec<LogId>>;

/// Tracks a named cursor for a given topic and persists it in the store.
#[derive(Clone, Debug)]
pub struct Acked {
    cursor_name: String,
    topic: Topic,
    store: SqliteStore,
    semaphore: Arc<Semaphore>,
}

impl Acked {
    /// Creates new `Acked` instance to track ack-state using the topic as a name.
    pub fn new(store: SqliteStore, topic: impl Into<Topic>) -> Self {
        let topic = topic.into();
        Self::from_name(store, topic, topic.to_string())
    }

    /// Creates new `Acked` instance with a custom name.
    ///
    /// This is useful if we want to have multiple instances using the same topic but tracking
    /// different states.
    pub fn from_name(store: SqliteStore, topic: impl Into<Topic>, name: impl AsRef<str>) -> Self {
        Self {
            store,
            topic: topic.into(),
            cursor_name: name.as_ref().to_string(),
            semaphore: Arc::new(Semaphore::new(1)),
        }
    }

    #[allow(unused)]
    pub fn cursor_name(&self) -> &str {
        &self.cursor_name
    }

    pub async fn cursor(&self) -> Result<Cursor<PublicKey, LogId>, AckedError> {
        let cursor = self.store.get_cursor(&self.cursor_name).await?;
        Ok(cursor.unwrap_or(Cursor::new(&self.cursor_name, LogHeights::default())))
    }

    async fn replace_cursor(
        &self,
        new_cursor: Cursor<PublicKey, LogId>,
    ) -> Result<Cursor<PublicKey, LogId>, AckedError> {
        // Fail if we try to use a cursor for a different acked state. This should help developers
        // to identify bugs.
        if new_cursor.name() != self.cursor_name {
            return Err(AckedError::InvalidName(
                new_cursor.name().to_owned(),
                self.cursor_name.to_owned(),
            ));
        }

        tx!(self.store, {
            self.store.set_cursor(&new_cursor).await?;
        });

        Ok(new_cursor)
    }

    /// Returns ranges of un-acked ("nacked") events which we might want to re-play.
    pub async fn nacked_log_ranges(
        &self,
        from: StreamFrom,
    ) -> Result<LogRanges<PublicKey, LogId>, AckedError> {
        let _permit = self.semaphore.acquire().await;

        // Get state vector of local replica for all logs related to this topic.
        let local_log_heights = {
            let logs: Logs = self.store.resolve(&self.topic).await?;
            get_log_heights(&self.store, &logs).await?
        };

        // Get cursor with state vector of "acked" operations.
        //
        // If a new cursor was given we replace the current one with it. This changes the persisted
        // state as well and can't be reversed!
        //
        // We do this to simplify the API, otherwise we would need to keep track of two cursors
        // (one for managing the replay, another for managing the stream itself).
        let cursor = match from {
            StreamFrom::Frontier => self.cursor().await?,
            StreamFrom::Start => {
                self.replace_cursor(Cursor::new(&self.cursor_name, LogHeights::default()))
                    .await?
            }
            StreamFrom::Cursor(cursor) => self.replace_cursor(cursor).await?,
        };

        // Compute difference between local set and what was acked so far. The result is the set of
        // all not-acked operations expressed as log ranges.
        let diff = cursor.compare(&local_log_heights);

        Ok(diff)
    }

    /// Advance internal cursor by acking an operation.
    pub async fn ack(&self, header: impl Borrow<Header>) -> Result<(), AckedError> {
        let _permit = self.semaphore.acquire().await;

        let header = header.borrow();

        // Make sure we're only acking operations for the given topic.
        if LogId::from_topic(self.topic) != header.extensions.log_id {
            return Err(AckedError::InvalidTopic(self.topic));
        }

        let mut cursor = self.cursor().await?;
        cursor.advance(header.public_key, header.extensions.log_id, header.seq_num);

        tx!(self.store, {
            self.store.set_cursor(&cursor).await?;
        });

        Ok(())
    }
}

impl std::hash::Hash for Acked {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.cursor_name.hash(state);
    }
}

impl PartialEq for Acked {
    fn eq(&self, other: &Self) -> bool {
        self.cursor_name == other.cursor_name && self.topic == other.topic
    }
}

impl Eq for Acked {}

async fn get_log_heights(
    store: &SqliteStore,
    logs: &Logs,
) -> Result<LogHeights<PublicKey, LogId>, SqliteError> {
    let mut result = BTreeMap::new();

    for (public_key, log_ids) in logs {
        let Some(log_heights) =
            LogStore::<Operation, PublicKey, LogId, SeqNum, Hash>::get_log_heights(
                store, public_key, log_ids,
            )
            .await?
        else {
            continue;
        };

        result.insert(*public_key, log_heights);
    }

    Ok(result)
}

#[derive(Debug, Error)]
pub enum AckedError {
    #[error("an error occurred while querying the store: {0}")]
    Store(#[from] SqliteError),

    #[error("can't use cursor with different name '{0}' for this stream, expected: {1}")]
    InvalidName(String, String),

    #[error("can't ack operation which is part of a different topic, expected: {0}")]
    InvalidTopic(Topic),
}

#[cfg(test)]
mod tests {
    use p2panda_core::Topic;
    use p2panda_store::SqliteStore;

    use crate::forge::{Forge, OperationForge};
    use crate::operation::{Extensions, LogId};
    use crate::streams::StreamFrom;

    use super::Acked;

    #[tokio::test]
    async fn nacked_log_ranges() {
        let topic = Topic::new();
        let store = SqliteStore::temporary().await;
        let forge = OperationForge::new(store.clone());
        let log_id = LogId::from_topic(topic);

        let acked = Acked::new(store.clone(), topic);

        // Expect name to be the same as topic.
        assert_eq!(acked.cursor_name(), topic.to_string());

        // There's nothing nacked yet.
        assert!(
            acked
                .nacked_log_ranges(StreamFrom::Frontier)
                .await
                .unwrap()
                .is_empty()
        );

        // Publish first operation.
        let operation_0 = forge
            .create_operation(
                topic,
                log_id,
                Some(b"la".to_vec()),
                Extensions::from_topic(topic),
            )
            .await
            .unwrap();

        // This first operation was not acked yet.
        let ranges = acked.nacked_log_ranges(StreamFrom::Frontier).await.unwrap();
        assert_eq!(
            ranges
                .get(&forge.public_key())
                .unwrap()
                .get(&log_id)
                .unwrap(),
            &(None, Some(0)),
        );

        // Ack it.
        acked.ack(operation_0).await.unwrap();
        assert!(
            acked
                .nacked_log_ranges(StreamFrom::Frontier)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn custom_name() {
        let topic = Topic::new();
        let store = SqliteStore::temporary().await;
        let forge = OperationForge::new(store.clone());
        let log_id = LogId::from_topic(topic);

        // We keep track of the same topic but with two independent "acked" cursors.
        let acked_1 = Acked::from_name(store.clone(), topic, "one");
        let acked_2 = Acked::from_name(store.clone(), topic, "two");

        assert_eq!(acked_1.cursor_name(), "one");
        assert_eq!(acked_2.cursor_name(), "two");

        let operation_0 = forge
            .create_operation(
                topic,
                log_id,
                Some(b"la".to_vec()),
                Extensions::from_topic(topic),
            )
            .await
            .unwrap();

        // The first cursor acks it.
        acked_1.ack(operation_0).await.unwrap();

        // Both cursors end up in different states.
        let ranges_1 = acked_1
            .nacked_log_ranges(StreamFrom::Frontier)
            .await
            .unwrap();
        assert!(ranges_1.is_empty());

        let ranges_2 = acked_2
            .nacked_log_ranges(StreamFrom::Frontier)
            .await
            .unwrap();
        assert_eq!(
            ranges_2
                .get(&forge.public_key())
                .unwrap()
                .get(&log_id)
                .unwrap(),
            &(None, Some(0)),
        );
    }

    #[tokio::test]
    async fn replaying_mutates_cursor_state() {
        let topic = Topic::new();
        let store = SqliteStore::temporary().await;
        let forge = OperationForge::new(store.clone());
        let log_id = LogId::from_topic(topic);

        let acked = Acked::new(store.clone(), topic);

        // Publish first operation and acknowledge it.
        let operation_0 = forge
            .create_operation(
                topic,
                log_id,
                Some(b"la".to_vec()),
                Extensions::from_topic(topic),
            )
            .await
            .unwrap();

        acked.ack(operation_0).await.unwrap();
        assert!(
            acked
                .nacked_log_ranges(StreamFrom::Frontier)
                .await
                .unwrap()
                .is_empty()
        );

        // Requesting to stream from the start will reset the internal state.
        let ranges = acked.nacked_log_ranges(StreamFrom::Start).await.unwrap();
        assert_eq!(
            ranges
                .get(&forge.public_key())
                .unwrap()
                .get(&log_id)
                .unwrap(),
            &(None, Some(0)),
        );

        // Do it again to show how it was persisted (the "frontier" was reset).
        let ranges = acked.nacked_log_ranges(StreamFrom::Frontier).await.unwrap();
        assert_eq!(
            ranges
                .get(&forge.public_key())
                .unwrap()
                .get(&log_id)
                .unwrap(),
            &(None, Some(0)),
        );
    }
}
