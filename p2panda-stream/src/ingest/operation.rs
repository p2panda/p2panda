// SPDX-License-Identifier: MIT OR Apache-2.0

//! Methods to handle p2panda operations.
use std::borrow::Borrow;

use p2panda_core::prune::validate_prunable_backlink;
use p2panda_core::{
    Extensions, Hash, LogId, Operation, OperationError, PublicKey, SeqNum, validate_operation,
};
use p2panda_store::Transaction;
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::topics::TopicStore;
use thiserror::Error;

/// Checks an incoming operation for log integrity and persists it into the store when valid.
///
/// Returns true if the operation was inserted to the store, false if the operation is valid but
/// already existed.
pub async fn ingest_operation<S, T, L, E, TP>(
    store: &S,
    operation: &T,
    log_id: L,
    topic: TP,
    prune_flag: bool,
) -> Result<bool, IngestError>
where
    S: Transaction
        + OperationStore<Operation<E>, Hash, L>
        + LogStore<Operation<E>, PublicKey, L, SeqNum, Hash>
        + TopicStore<TP, PublicKey, L>,
    // TODO: remove Clone after https://github.com/p2panda/p2panda/issues/1040
    T: Borrow<Operation<E>> + Clone,
    L: LogId,
    E: Extensions,
{
    let operation: &Operation<E> = operation.borrow();

    // Validate operation format.
    validate_operation(operation)?;

    let permit = store
        .begin()
        .await
        .map_err(|err| IngestError::StoreError(err.to_string()))?;

    // Ignore insertion if operation already exists.
    let already_exists = store
        .has_operation_tx(&operation.hash)
        .await
        .map_err(|err| IngestError::StoreError(err.to_string()))?;
    if already_exists {
        return Ok(false);
    }

    // Validate log integrity.
    let past_header = {
        // TODO: This is not returning the "latest entry". See related issue:
        // https://github.com/p2panda/p2panda/issues/1039
        let latest = store
            .get_latest_entry_tx(&operation.header.public_key, &log_id)
            .await
            .map_err(|err| IngestError::StoreError(err.to_string()))?;

        let latest_operation = match latest {
            Some(latest) => store
                .get_operation_tx(&latest.0)
                .await
                .map_err(|err| IngestError::StoreError(err.to_string()))?,
            None => None,
        };

        latest_operation.map(|operation| operation.header)
    };

    // If no pruning flag is set, we expect the log to have integrity with the previously given
    // operation.
    validate_prunable_backlink(past_header.as_ref(), &operation.header, prune_flag)?;

    // Insert operation into store and associate its log with the given topic.
    let id = operation.hash;
    let public_key = operation.header.public_key;

    store
        .insert_operation(&id, operation.to_owned(), log_id.clone())
        .await
        .map_err(|err| IngestError::StoreError(err.to_string()))?;

    <S as TopicStore<TP, PublicKey, L>>::associate(store, &topic, &public_key, &log_id)
        .await
        .map_err(|err| IngestError::StoreError(err.to_string()))?;

    store
        .commit(permit)
        .await
        .map_err(|err| IngestError::StoreError(err.to_string()))?;

    Ok(true)
}

/// Errors which can occur due to invalid operations or critical storage failures.
#[derive(Clone, Debug, Error)]
pub enum IngestError {
    /// Operation can not be authenticated, has broken log- or payload integrity or doesn't follow
    /// the p2panda specification.
    #[error(transparent)]
    InvalidOperation(#[from] OperationError),

    /// Critical storage failure occurred. This is usually a reason to panic.
    #[error("critical storage failure: {0}")]
    StoreError(String),
}

#[cfg(test)]
mod tests {
    use p2panda_core::test_utils::TestLog;
    use p2panda_core::{Hash, Header, Operation, PrivateKey, PublicKey, SeqNum, Timestamp};
    use p2panda_store::SqliteStore;
    use p2panda_store::logs::LogStore;
    use p2panda_store::topics::TopicStore;

    use super::ingest_operation;

    #[tokio::test]
    async fn valid_log() {
        let store = SqliteStore::temporary().await;
        let log = TestLog::new();

        for i in 0..128 {
            let operation = log.operation(format!("{i}").as_bytes(), ());
            let result = ingest_operation(&store, &operation, 1, 1, false).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn deduplicate_operations() {
        let store = SqliteStore::temporary().await;
        let log = TestLog::new();
        let operation = log.operation(b"same same", ());

        let result = ingest_operation(&store, &operation, 1, 1, false)
            .await
            .unwrap();
        assert!(result);

        // Inserting duplicates is ok and are silently ignored.
        let result = ingest_operation(&store, &operation, 1, 1, false)
            .await
            .unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn topic_association() {
        let store = SqliteStore::temporary().await;

        let log_0 = TestLog::new();
        let log_1 = TestLog::new();
        let log_2 = TestLog::new();

        let dogs = [2; 32];
        let cats = [3; 32];

        ingest_operation(&store, &log_0.operation(b"Do", ()), 0, dogs, false)
            .await
            .unwrap();

        ingest_operation(&store, &log_0.operation(b"Re", ()), 0, dogs, false)
            .await
            .unwrap();

        ingest_operation(&store, &log_1.operation(b"Mi", ()), 1, dogs, false)
            .await
            .unwrap();

        ingest_operation(&store, &log_2.operation(b"Fa", ()), 2, cats, false)
            .await
            .unwrap();

        ingest_operation(&store, &log_2.operation(b"So", ()), 2, cats, false)
            .await
            .unwrap();

        ingest_operation(&store, &log_2.operation(b"La", ()), 2, cats, false)
            .await
            .unwrap();

        // Topic "dogs" contains two logs: 0 with two operations and 1 with one operation.
        let authors =
            <SqliteStore<'_> as TopicStore<[u8; 32], PublicKey, usize>>::resolve(&store, &dogs)
                .await
                .unwrap();
        assert_eq!(*authors.get(&log_0.author()).unwrap(), [0]);
        assert_eq!(*authors.get(&log_1.author()).unwrap(), [1]);

        let (_hash, seq_num) = <SqliteStore<'_> as LogStore<
            Operation<()>,
            PublicKey,
            usize,
            SeqNum,
            Hash,
        >>::get_latest_entry(&store, &log_0.author(), &0)
        .await
        .unwrap()
        .unwrap();
        assert_eq!(seq_num, 1);

        // Topic "cats" contains one log: 2 with four operations.
        let authors =
            <SqliteStore<'_> as TopicStore<[u8; 32], PublicKey, usize>>::resolve(&store, &cats)
                .await
                .unwrap();
        assert_eq!(*authors.get(&log_2.author()).unwrap(), [2]);

        let (_hash, seq_num) = <SqliteStore<'_> as LogStore<
            Operation<()>,
            PublicKey,
            usize,
            SeqNum,
            Hash,
        >>::get_latest_entry(&store, &log_2.author(), &2)
        .await
        .unwrap()
        .unwrap();
        assert_eq!(seq_num, 2);
    }

    #[tokio::test]
    async fn missing_prefix() {
        let store = SqliteStore::temporary().await;
        let private_key = PrivateKey::new();

        // Create an operation which has already advanced in the log (it has a backlink and higher
        // sequence number).
        let mut header = Header {
            public_key: private_key.public_key(),
            version: 1,
            signature: None,
            payload_size: 0,
            payload_hash: None,
            timestamp: Timestamp::now(),
            seq_num: 12, // we'll be missing 11 operations between the first and this one
            backlink: Some(Hash::new(b"mock operation")),
            extensions: (),
        };
        header.sign(&private_key);

        let operation = Operation {
            hash: header.hash(),
            header,
            body: None,
        };

        let result = ingest_operation(&store, &operation, 1, 1, false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn ignore_outdated_pruned_operations() {
        let store = SqliteStore::temporary().await;
        let private_key = PrivateKey::new();

        // 1. Create an advanced operation in a log which assumes that all previous operations have
        //    been pruned.
        let mut header = Header {
            public_key: private_key.public_key(),
            version: 1,
            signature: None,
            payload_size: 0,
            payload_hash: None,
            timestamp: Timestamp::now(),
            seq_num: 1,
            backlink: Some(Hash::new(b"mock operation")),
            extensions: (),
        };
        header.sign(&private_key);

        let operation = Operation {
            hash: header.hash(),
            header,
            body: None,
        };

        let prune_flag = true; // Ingest does not do any pruning, but the flag affects validation.
        let result = ingest_operation(&store, &operation, 1, 1, prune_flag).await;
        assert!(result.is_ok());

        // 2. Create an operation which is from an "outdated" seq from before the log was pruned.
        let mut header = Header {
            public_key: private_key.public_key(),
            version: 1,
            signature: None,
            payload_size: 0,
            payload_hash: None,
            timestamp: Timestamp::now(),
            seq_num: 0,
            backlink: None,
            extensions: (),
        };
        header.sign(&private_key);

        let operation = Operation {
            hash: header.hash(),
            header,
            body: None,
        };

        let result = ingest_operation(&store, &operation, 1, 1, false).await;
        assert!(result.is_err());
    }
}
