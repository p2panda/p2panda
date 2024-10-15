// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use p2panda_core::extensions::DefaultExtensions;
use p2panda_core::{Hash, Operation, PublicKey};

use crate::traits::{OperationStore, RawStore, StoreError};
use crate::LogStore;

type SeqNum = u64;
type Timestamp = u64;
type LogMeta = (SeqNum, Timestamp, Hash);

#[derive(Clone, Debug)]
pub struct InnerMemoryStore<T, E> {
    operations: HashMap<Hash, Operation<E>>,
    raw: HashMap<Hash, (Vec<u8>, Option<Vec<u8>>)>,
    logs: HashMap<(PublicKey, T), BTreeSet<LogMeta>>,
}

#[derive(Clone, Debug)]
pub struct MemoryStore<T, E> {
    inner: Arc<RwLock<InnerMemoryStore<T, E>>>,
}

impl<T, E> MemoryStore<T, E> {
    pub fn new() -> Self {
        let inner = InnerMemoryStore {
            operations: HashMap::new(),
            raw: HashMap::new(),
            logs: HashMap::new(),
        };

        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }
}

impl<T> Default for MemoryStore<T, DefaultExtensions> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, E> MemoryStore<T, E> {
    pub fn read_store(&self) -> RwLockReadGuard<InnerMemoryStore<T, E>> {
        self.inner
            .read()
            .expect("acquire shared read access on store")
    }

    pub fn write_store(&self) -> RwLockWriteGuard<InnerMemoryStore<T, E>> {
        self.inner
            .write()
            .expect("acquire exclusive write access on store")
    }
}

impl<T, E> OperationStore<T, E> for MemoryStore<T, E>
where
    T: Clone + Send + Sync + Eq + std::hash::Hash + Default + std::fmt::Debug,
    E: Clone + Send + Sync,
{
    async fn insert_operation(
        &mut self,
        operation: &Operation<E>,
        log_id: &T,
    ) -> Result<bool, StoreError> {
        let entry = (
            operation.header.seq_num,
            operation.header.timestamp,
            operation.hash,
        );

        let mut store = self.write_store();
        let insertion_occured = store
            .logs
            .entry((operation.header.public_key, log_id.to_owned()))
            .or_default()
            .insert(entry);

        if insertion_occured {
            store
                .operations
                .insert(operation.hash, operation.to_owned());
        }

        Ok(insertion_occured)
    }

    async fn get_operation(&self, hash: Hash) -> Result<Option<Operation<E>>, StoreError> {
        Ok(self.read_store().operations.get(&hash).cloned())
    }

    async fn delete_operation(&mut self, hash: Hash) -> Result<bool, StoreError> {
        let mut store = self.write_store();
        let Some(removed) = store.operations.remove(&hash) else {
            return Ok(false);
        };
        store.logs = store
            .logs
            .clone()
            .into_iter()
            .filter_map(|(key, mut log)| {
                log.remove(&(
                    removed.header.seq_num,
                    removed.header.timestamp,
                    removed.hash,
                ));
                if log.is_empty() {
                    None
                } else {
                    Some((key, log))
                }
            })
            .collect();

        Ok(true)
    }

    async fn delete_payload(&mut self, hash: Hash) -> Result<bool, StoreError> {
        if let Some(operation) = self.write_store().operations.get_mut(&hash) {
            operation.body = None;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl<T, E> RawStore<E> for MemoryStore<T, E>
where
    T: Send + Sync,
    E: Clone + Send + Sync,
{
    async fn insert_raw_operation(
        &mut self,
        hash: Hash,
        header_bytes: &[u8],
        body_bytes: Option<&[u8]>,
    ) -> Result<bool, StoreError> {
        let mut store = self.write_store();
        let insertion_occured = store
            .raw
            .insert(
                hash,
                (
                    header_bytes.to_vec(),
                    body_bytes.map(|bytes| bytes.to_vec()),
                ),
            )
            .is_none();
        Ok(insertion_occured)
    }

    async fn get_raw_operation(
        &self,
        hash: Hash,
    ) -> Result<Option<(Vec<u8>, Option<Vec<u8>>)>, StoreError> {
        let store = self.read_store();
        Ok(store.raw.get(&hash).cloned())
    }
}

impl<T, E> LogStore<T, E> for MemoryStore<T, E>
where
    T: Clone + Send + Sync + Eq + std::hash::Hash + Default + std::fmt::Debug,
    E: Clone + Send + Sync,
{
    async fn get_log(
        &self,
        public_key: &PublicKey,
        log_id: &T,
    ) -> Result<Vec<Operation<E>>, StoreError> {
        let mut operations = Vec::new();
        let store = self.read_store();
        if let Some(log) = store.logs.get(&(*public_key, log_id.to_owned())) {
            for (_, _, hash) in log {
                let operation = store
                    .operations
                    .get(hash)
                    .expect("operation exists in hashmap");
                operations.push(operation.clone())
            }
        };
        Ok(operations)
    }

    async fn latest_operation(
        &self,
        public_key: &PublicKey,
        log_id: &T,
    ) -> Result<Option<Operation<E>>, StoreError> {
        let store = self.read_store();
        let latest = match store.logs.get(&(*public_key, log_id.to_owned())) {
            Some(log) => match log.last() {
                Some((_, _, hash)) => {
                    let operation = store.operations.get(hash);
                    operation.cloned()
                }
                None => None,
            },
            None => None,
        };
        Ok(latest)
    }

    async fn delete_operations(
        &mut self,
        public_key: &PublicKey,
        log_id: &T,
        before: u64,
    ) -> Result<bool, StoreError> {
        let mut deleted = vec![];

        let mut store = self.write_store();
        if let Some(log) = store.logs.get_mut(&(*public_key, log_id.to_owned())) {
            log.retain(|(seq_num, _, hash)| {
                let remove = *seq_num < before;
                if remove {
                    deleted.push(*hash);
                };
                !remove
            });
        };
        store.operations.retain(|hash, _| !deleted.contains(hash));
        Ok(!deleted.is_empty())
    }

    async fn delete_payloads(
        &mut self,
        public_key: &PublicKey,
        log_id: &T,
        from: u64,
        to: u64,
    ) -> Result<bool, StoreError> {
        let mut deleted = vec![];
        {
            let store = self.read_store();
            if let Some(log) = store.logs.get(&(*public_key, log_id.to_owned())) {
                log.iter().for_each(|(seq_num, _, hash)| {
                    if *seq_num >= from && *seq_num < to {
                        deleted.push(*hash)
                    };
                });
            };
        }
        let mut store = self.write_store();
        for hash in &deleted {
            let operation = store
                .operations
                .get_mut(hash)
                .expect("operation exists in store");
            operation.body = None;
        }
        Ok(!deleted.is_empty())
    }

    async fn get_log_heights(&self, log_id: &T) -> Result<Vec<(PublicKey, SeqNum)>, StoreError> {
        let log_heights = self
            .read_store()
            .logs
            .iter()
            .filter_map(|((public_key, inner_log_id), log)| {
                if inner_log_id == log_id {
                    let log_height = log
                        .last()
                        .expect("all logs contain at least one operation")
                        .0;
                    Some((*public_key, log_height))
                } else {
                    None
                }
            })
            .collect();
        Ok(log_heights)
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::{Body, Hash, Header, Operation, PrivateKey};
    use serde::{Deserialize, Serialize};

    use crate::{LogStore, OperationStore};

    use super::MemoryStore;

    fn generate_operation(
        private_key: &PrivateKey,
        body: Body,
        seq_num: u64,
        timestamp: u64,
        backlink: Option<Hash>,
    ) -> Operation {
        let mut header = Header {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp,
            seq_num,
            backlink,
            previous: vec![],
            extensions: None,
        };
        header.sign(&private_key);

        Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        }
    }

    #[tokio::test]
    async fn default_memory_store() {
        let mut store = MemoryStore::default();
        let private_key = PrivateKey::new();
        let body = Body::new("hello!".as_bytes());

        let operation = generate_operation(&private_key, body, 0, 0, None);
        let inserted = store
            .insert_operation(&operation, &0)
            .await
            .expect("no errors");
        assert!(inserted);
    }

    #[tokio::test]
    async fn generic_extensions_mem_store() {
        // Define our own custom extension type
        #[derive(Clone, Serialize, Deserialize)]
        struct MyExtension {}

        // Construct a new store
        let mut store = MemoryStore::new();

        // Construct an operation using the custom extension
        let private_key = PrivateKey::new();
        let body = Body::new("hello!".as_bytes());
        let mut header = Header {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![],
            extensions: Some(MyExtension {}),
        };
        header.sign(&private_key);

        let operation = Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        };

        // Insert the operation into the store, the extension type is inferred
        let inserted = store
            .insert_operation(&operation, &0)
            .await
            .expect("no errors");
        assert!(inserted);
    }

    #[tokio::test]
    async fn insert_get_operation() {
        let mut store = MemoryStore::default();
        let private_key = PrivateKey::new();
        let body = Body::new("hello!".as_bytes());

        let operation = generate_operation(&private_key, body, 0, 0, None);

        // Insert one operation
        let inserted = store
            .insert_operation(&operation, &0)
            .await
            .expect("no errors");
        assert!(inserted);

        // Retrieve it agin
        let retreived_operation = store
            .get_operation(operation.hash)
            .await
            .expect("no error")
            .expect("operation exists");

        assert_eq!(operation, retreived_operation);
    }

    #[tokio::test]
    async fn delete_operation() {
        let mut store: MemoryStore<i32, p2panda_core::extensions::DefaultExtensions> =
            MemoryStore::default();
        let private_key = PrivateKey::new();
        let body = Body::new("hello!".as_bytes());

        let operation = generate_operation(&private_key, body, 0, 0, None);

        // Insert one operation
        let inserted = store
            .insert_operation(&operation, &0)
            .await
            .expect("no errors");
        assert!(inserted);

        // We expect one log and one operation
        assert_eq!(store.read_store().logs.len(), 1);
        assert_eq!(store.read_store().operations.len(), 1);

        // Delete the operation
        assert!(store
            .delete_operation(operation.hash)
            .await
            .expect("no error"));

        // We expect no logs and no operations
        assert_eq!(store.read_store().logs.len(), 0);
        assert_eq!(store.read_store().operations.len(), 0);

        // Try to get the operation
        let deleted_operation = store.get_operation(operation.hash).await.expect("no error");

        // It isn't there anymore
        assert!(deleted_operation.is_none());
    }

    #[tokio::test]
    async fn delete_payload() {
        let mut store: MemoryStore<i32, p2panda_core::extensions::DefaultExtensions> =
            MemoryStore::default();
        let private_key = PrivateKey::new();
        let body = Body::new("hello!".as_bytes());

        let operation = generate_operation(&private_key, body, 0, 0, None);

        // Insert one operation
        let inserted = store
            .insert_operation(&operation, &0)
            .await
            .expect("no errors");
        assert!(inserted);

        // Delete the payload
        assert!(store
            .delete_payload(operation.hash)
            .await
            .expect("no error"));

        // Retrieve the operation again
        let operation_no_payload = store
            .get_operation(operation.hash)
            .await
            .expect("no error")
            .expect("operation exists");

        // The value of body is `None`
        assert!(operation_no_payload.body.is_none());
    }

    #[tokio::test]
    async fn get_log() {
        let mut store = MemoryStore::default();
        let private_key = PrivateKey::new();
        let log_id = 0;

        let body0 = Body::new("hello!".as_bytes());
        let body1 = Body::new("hello again!".as_bytes());

        let operation_0 = generate_operation(&private_key, body0, 0, 0, None);
        let operation_1 = generate_operation(&private_key, body1, 1, 0, Some(operation_0.hash));

        store
            .insert_operation(&operation_0, &log_id)
            .await
            .expect("no errors");
        store
            .insert_operation(&operation_1, &log_id)
            .await
            .expect("no errors");

        let log = store
            .get_log(&private_key.public_key(), &log_id)
            .await
            .expect("no errors");

        assert_eq!(log.len(), 2);
        assert_eq!(log[0], operation_0);
        assert_eq!(log[1], operation_1);
    }

    #[tokio::test]
    async fn insert_many_get_one_log() {
        let mut store = MemoryStore::default();
        let private_key = PrivateKey::new();
        let log_a_id = "a";
        let log_b_id = "b";

        let body_a0 = Body::new("hello from log a!".as_bytes());
        let body_a1 = Body::new("hello from log a again!".as_bytes());
        let log_a_operation_0 = generate_operation(&private_key, body_a0, 0, 0, None);
        let log_a_operation_1 =
            generate_operation(&private_key, body_a1, 1, 1, Some(log_a_operation_0.hash));

        let inserted = store
            .insert_operation(&log_a_operation_0, &log_a_id)
            .await
            .expect("no errors");
        assert!(inserted);

        let inserted = store
            .insert_operation(&log_a_operation_1, &log_a_id)
            .await
            .expect("no errors");
        assert!(inserted);

        let body_b0 = Body::new("hello from log b!".as_bytes());
        let body_b1 = Body::new("hello from log b again!".as_bytes());
        let log_b_operation_0 = generate_operation(&private_key, body_b0, 0, 3, None);
        let log_b_operation_1 =
            generate_operation(&private_key, body_b1, 1, 4, Some(log_b_operation_0.hash));

        store
            .insert_operation(&log_b_operation_0, &log_b_id)
            .await
            .expect("no errors");

        store
            .insert_operation(&log_b_operation_1, &log_b_id)
            .await
            .expect("no errors");

        let log_a = store
            .get_log(&private_key.public_key(), &log_a_id)
            .await
            .expect("no errors");

        assert_eq!(log_a.len(), 2);
        assert_eq!(log_a[0], log_a_operation_0);
        assert_eq!(log_a[1], log_a_operation_1);

        let log_b = store
            .get_log(&private_key.public_key(), &log_b_id)
            .await
            .expect("no errors");

        assert_eq!(log_b.len(), 2);
        assert_eq!(log_b[0], log_b_operation_0);
        assert_eq!(log_b[1], log_b_operation_1);
    }

    #[tokio::test]
    async fn many_authors_same_log_id() {
        let mut store = MemoryStore::default();
        let private_key_a = PrivateKey::new();
        let private_key_b = PrivateKey::new();
        let log_id = 0;
        let body = Body::new("hello!".as_bytes());

        let author_a_operation = generate_operation(&private_key_a, body.clone(), 0, 0, None);
        let inserted = store
            .insert_operation(&author_a_operation, &log_id)
            .await
            .expect("no errors");
        assert!(inserted);

        let author_b_operation = generate_operation(&private_key_b, body, 0, 0, None);
        let inserted = store
            .insert_operation(&author_b_operation, &log_id)
            .await
            .expect("no errors");
        assert!(inserted);

        let author_a_log = store
            .get_log(&private_key_a.public_key(), &log_id)
            .await
            .expect("no errors");

        assert_eq!(author_a_log.len(), 1);
        assert_eq!(author_a_log[0], author_a_operation);

        let author_b_log = store
            .get_log(&private_key_b.public_key(), &log_id)
            .await
            .expect("no errors");

        assert_eq!(author_b_log.len(), 1);
        assert_eq!(author_b_log[0], author_b_operation);
    }

    #[tokio::test]
    async fn get_latest_operation() {
        let mut store = MemoryStore::default();
        let private_key = PrivateKey::new();
        let log_id = 0;

        let body0 = Body::new("hello!".as_bytes());
        let body1 = Body::new("hello again!".as_bytes());

        let operation_0 = generate_operation(&private_key, body0, 0, 0, None);
        let operation_1 = generate_operation(&private_key, body1, 1, 0, Some(operation_0.hash));

        store
            .insert_operation(&operation_0, &log_id)
            .await
            .expect("no errors");
        store
            .insert_operation(&operation_1, &log_id)
            .await
            .expect("no errors");

        let latest_operation = store
            .latest_operation(&private_key.public_key(), &log_id)
            .await
            .expect("no errors");

        assert_eq!(latest_operation, Some(operation_1));
    }

    #[tokio::test]
    async fn delete_operations() {
        let mut store = MemoryStore::default();
        let private_key = PrivateKey::new();
        let log_id = 0;

        let body0 = Body::new("hello!".as_bytes());
        let body1 = Body::new("hello again!".as_bytes());
        let body2 = Body::new("final hello!".as_bytes());

        let operation_0 = generate_operation(&private_key, body0, 0, 0, None);
        let operation_1 = generate_operation(&private_key, body1, 1, 100, Some(operation_0.hash));
        let operation_2 = generate_operation(&private_key, body2, 2, 200, Some(operation_0.hash));

        store
            .insert_operation(&operation_0, &log_id)
            .await
            .expect("no errors");
        store
            .insert_operation(&operation_1, &log_id)
            .await
            .expect("no errors");
        store
            .insert_operation(&operation_2, &log_id)
            .await
            .expect("no errors");

        // We expect one log and 3 operations
        assert_eq!(store.read_store().logs.len(), 1);
        assert_eq!(store.read_store().operations.len(), 3);

        // Delete all operations _before_ seq_num 2
        let deleted = store
            .delete_operations(&private_key.public_key(), &log_id, 2)
            .await
            .expect("no errors");
        assert!(deleted);

        // There is now only one operation in the log
        assert_eq!(store.read_store().logs.len(), 1);
        assert_eq!(store.read_store().operations.len(), 1);

        // The remaining operation in the log should be the latest (seq_num == 2)
        let log = store
            .get_log(&private_key.public_key(), &log_id)
            .await
            .expect("no errors");
        assert_eq!(log[0], operation_2);

        // Deleting the same range again should return `false`, meaning no deletion occurred
        let deleted = store
            .delete_operations(&private_key.public_key(), &log_id, 2)
            .await
            .expect("no errors");
        assert!(!deleted);
    }

    #[tokio::test]
    async fn delete_payloads() {
        let mut store = MemoryStore::default();
        let private_key = PrivateKey::new();
        let log_id = 0;

        let body0 = Body::new("hello!".as_bytes());
        let body1 = Body::new("hello again!".as_bytes());
        let body2 = Body::new("final hello!".as_bytes());

        let operation_0 = generate_operation(&private_key, body0, 0, 0, None);
        let operation_1 = generate_operation(&private_key, body1, 1, 100, Some(operation_0.hash));
        let operation_2 =
            generate_operation(&private_key, body2.clone(), 2, 200, Some(operation_1.hash));

        store
            .insert_operation(&operation_0, &log_id)
            .await
            .expect("no errors");
        store
            .insert_operation(&operation_1, &log_id)
            .await
            .expect("no errors");
        store
            .insert_operation(&operation_2, &log_id)
            .await
            .expect("no errors");

        // There is one log and 3 operations
        assert_eq!(store.read_store().logs.len(), 1);
        assert_eq!(store.read_store().operations.len(), 3);

        // Delete all operation payloads from sequence number 0 up to but not including 2
        let deleted = store
            .delete_payloads(&private_key.public_key(), &log_id, 0, 2)
            .await
            .expect("no errors");
        assert!(deleted);

        let log = store
            .get_log(&private_key.public_key(), &log_id)
            .await
            .expect("no errors");

        assert_eq!(log[0].body, None);
        assert_eq!(log[1].body, None);
        assert_eq!(log[2].body, Some(body2));
    }
}
