// SPDX-License-Identifier: MIT OR Apache-2.0

//! In-memory persistence for p2panda operations and logs.
use std::collections::{BTreeSet, HashMap};
use std::convert::Infallible;
use std::fmt::Debug;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use p2panda_core::{Body, Extensions, Hash, Header, PublicKey, RawOperation};

use crate::{LogId, LogStore, OperationStore};

type SeqNum = u64;
type Timestamp = u64;
type RawHeader = Vec<u8>;

type LogMeta = (SeqNum, Timestamp, Hash);
type StoredOperation<L, E> = (L, Header<E>, Option<Body>, RawHeader);

/// An in-memory store for core p2panda data types: `Operation` and `Log`.
#[derive(Clone, Debug)]
pub struct InnerMemoryStore<L, E> {
    operations: HashMap<Hash, StoredOperation<L, E>>,
    logs: HashMap<(PublicKey, L), BTreeSet<LogMeta>>,
}

/// An in-memory store for core p2panda data types: `Operation` and log.
///
/// `MemoryStore` supports usage in asynchronous and multi-threaded contexts by wrapping an
/// `InnerMemoryStore` with an `RwLock` and `Arc`. Convenience methods are provided to obtain a
/// read- or write-lock on the underlying store.
#[derive(Clone, Debug)]
pub struct MemoryStore<L, E = ()> {
    inner: Arc<RwLock<InnerMemoryStore<L, E>>>,
}

impl<L, E> MemoryStore<L, E> {
    /// Create a new in-memory store.
    pub fn new() -> Self {
        let inner = InnerMemoryStore {
            operations: HashMap::new(),
            logs: HashMap::new(),
        };

        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }
}

impl<T> Default for MemoryStore<T, ()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, E> MemoryStore<T, E> {
    /// Obtain a read-lock on the store.
    pub fn read_store(&self) -> RwLockReadGuard<InnerMemoryStore<T, E>> {
        self.inner
            .read()
            .expect("acquire shared read access on store")
    }

    /// Obtain a write-lock on the store.
    pub fn write_store(&self) -> RwLockWriteGuard<InnerMemoryStore<T, E>> {
        self.inner
            .write()
            .expect("acquire exclusive write access on store")
    }
}

impl<L, E> OperationStore<L, E> for MemoryStore<L, E>
where
    L: LogId + Send + Sync,
    E: Extensions + Send + Sync,
{
    type Error = Infallible;

    async fn insert_operation(
        &mut self,
        hash: Hash,
        header: &Header<E>,
        body: Option<&Body>,
        header_bytes: &[u8],
        log_id: &L,
    ) -> Result<bool, Self::Error> {
        let mut store = self.write_store();

        let log_meta = (header.seq_num, header.timestamp, hash);
        let insertion_occured = store
            .logs
            .entry((header.public_key, log_id.to_owned()))
            .or_default()
            .insert(log_meta);

        if insertion_occured {
            let entry = (
                log_id.to_owned(),
                header.to_owned(),
                body.cloned(),
                header_bytes.to_vec(),
            );
            store.operations.insert(hash, entry);
        }

        Ok(insertion_occured)
    }

    async fn get_operation(
        &self,
        hash: Hash,
    ) -> Result<Option<(Header<E>, Option<Body>)>, Self::Error> {
        match self.read_store().operations.get(&hash) {
            Some((_, header, body, _)) => Ok(Some((header.clone(), body.clone()))),
            None => Ok(None),
        }
    }

    async fn get_raw_operation(&self, hash: Hash) -> Result<Option<RawOperation>, Self::Error> {
        match self.read_store().operations.get(&hash) {
            Some((_, _, body, header_bytes)) => Ok(Some((
                header_bytes.clone(),
                body.as_ref().map(|body| body.to_bytes()),
            ))),
            None => Ok(None),
        }
    }

    async fn has_operation(&self, hash: Hash) -> Result<bool, Self::Error> {
        Ok(self.read_store().operations.contains_key(&hash))
    }

    async fn delete_operation(&mut self, hash: Hash) -> Result<bool, Self::Error> {
        let mut store = self.write_store();
        let Some((_, header, _, _)) = store.operations.remove(&hash) else {
            return Ok(false);
        };
        store.logs = store
            .logs
            .clone()
            .into_iter()
            .filter_map(|(key, mut log)| {
                log.remove(&(header.seq_num, header.timestamp, hash));
                if log.is_empty() {
                    None
                } else {
                    Some((key, log))
                }
            })
            .collect();

        Ok(true)
    }

    async fn delete_payload(&mut self, hash: Hash) -> Result<bool, Self::Error> {
        if let Some(operation) = self.write_store().operations.get_mut(&hash) {
            operation.2 = None;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl<L, E> LogStore<L, E> for MemoryStore<L, E>
where
    L: LogId + Send + Sync,
    E: Extensions + Send + Sync,
{
    type Error = Infallible;

    async fn get_log(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        from: Option<u64>,
    ) -> Result<Option<Vec<(Header<E>, Option<Body>)>>, Self::Error> {
        let store = self.read_store();
        match store.logs.get(&(*public_key, log_id.to_owned())) {
            Some(log) => {
                let mut result = Vec::new();
                if let Some(from) = from {
                    log.iter().for_each(|(seq_num, _, hash)| {
                        if *seq_num >= from {
                            let (_, header, body, _) =
                                store.operations.get(hash).expect("exists in hash map");
                            result.push((header.to_owned(), body.to_owned()));
                        }
                    });
                } else {
                    log.iter().for_each(|(_, _, hash)| {
                        let (_, header, body, _) =
                            store.operations.get(hash).expect("exists in hash map");
                        result.push((header.to_owned(), body.to_owned()));
                    });
                }
                Ok(Some(result))
            }
            None => Ok(None),
        }
    }

    async fn get_raw_log(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        from: Option<u64>,
    ) -> Result<Option<Vec<RawOperation>>, Self::Error> {
        let store = self.read_store();
        match store.logs.get(&(*public_key, log_id.to_owned())) {
            Some(log) => {
                let mut result = Vec::new();
                if let Some(from) = from {
                    log.iter().for_each(|(seq_num, _, hash)| {
                        if *seq_num >= from {
                            let (_, _, body, header_bytes) =
                                store.operations.get(hash).expect("exists in hash map");
                            result.push((
                                header_bytes.clone(),
                                body.as_ref().map(|body| body.to_bytes()),
                            ));
                        }
                    });
                } else {
                    log.iter().for_each(|(_, _, hash)| {
                        let (_, _, body, header_bytes) =
                            store.operations.get(hash).expect("exists in hash map");
                        result.push((
                            header_bytes.clone(),
                            body.as_ref().map(|body| body.to_bytes()),
                        ));
                    });
                }
                Ok(Some(result))
            }
            None => Ok(None),
        }
    }

    async fn latest_operation(
        &self,
        public_key: &PublicKey,
        log_id: &L,
    ) -> Result<Option<(Header<E>, Option<Body>)>, Self::Error> {
        let store = self.read_store();

        let Some(log) = store.logs.get(&(*public_key, log_id.to_owned())) else {
            return Ok(None);
        };

        let Some((_, _, hash)) = log.last() else {
            return Ok(None);
        };

        let Some((_, header, body, _)) = store.operations.get(hash) else {
            return Ok(None);
        };

        Ok(Some((header.to_owned(), body.to_owned())))
    }

    async fn delete_operations(
        &mut self,
        public_key: &PublicKey,
        log_id: &L,
        before: u64,
    ) -> Result<bool, Self::Error> {
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
        log_id: &L,
        from: u64,
        to: u64,
    ) -> Result<bool, Self::Error> {
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
            operation.2 = None;
        }
        Ok(!deleted.is_empty())
    }

    async fn get_log_heights(&self, log_id: &L) -> Result<Vec<(PublicKey, SeqNum)>, Self::Error> {
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
    use p2panda_core::{Body, Hash, Header, PrivateKey};
    use serde::{Deserialize, Serialize};

    use crate::{LogStore, OperationStore};

    use super::MemoryStore;

    fn create_operation(
        private_key: &PrivateKey,
        body: &Body,
        seq_num: u64,
        timestamp: u64,
        backlink: Option<Hash>,
    ) -> (Hash, Header<()>, Vec<u8>) {
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
        header.sign(private_key);
        let header_bytes = header.to_bytes();
        (header.hash(), header, header_bytes)
    }

    #[tokio::test]
    async fn default_memory_store() {
        let mut store = MemoryStore::default();
        let private_key = PrivateKey::new();
        let body = Body::new("hello!".as_bytes());

        let (hash, header, header_bytes) = create_operation(&private_key, &body, 0, 0, None);
        let inserted = store
            .insert_operation(hash, &header, Some(&body), &header_bytes, &0)
            .await
            .expect("no errors");
        assert!(inserted);
    }

    #[tokio::test]
    async fn generic_extensions_mem_store() {
        // Define our own custom extension type.
        #[derive(Clone, Debug, Default, Serialize, Deserialize)]
        struct MyExtension {}

        // Construct a new store.
        let mut store = MemoryStore::new();

        // Construct an operation using the custom extension.
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

        // Insert the operation into the store, the extension type is inferred.
        let inserted = store
            .insert_operation(header.hash(), &header, Some(&body), &header.to_bytes(), &0)
            .await
            .expect("no errors");
        assert!(inserted);
    }

    #[tokio::test]
    async fn insert_get_operation() {
        let mut store = MemoryStore::default();
        let private_key = PrivateKey::new();
        let body = Body::new("hello!".as_bytes());

        let (hash, header, header_bytes) = create_operation(&private_key, &body, 0, 0, None);

        let inserted = store
            .insert_operation(hash, &header, Some(&body), &header_bytes, &0)
            .await
            .expect("no errors");
        assert!(inserted);
        assert!(store.has_operation(hash).await.expect("no error"));

        let (header_again, body_again) = store
            .get_operation(hash)
            .await
            .expect("no error")
            .expect("operation exist");

        assert_eq!(header.hash(), header_again.hash());
        assert_eq!(Some(body.clone()), body_again);

        let (header_bytes_again, body_bytes_again) = store
            .get_raw_operation(hash)
            .await
            .expect("no error")
            .expect("operation exist");

        assert_eq!(header_bytes_again, header_bytes);
        assert_eq!(body_bytes_again, Some(body.to_bytes()));
    }

    #[tokio::test]
    async fn delete_operation() {
        let mut store: MemoryStore<i32> = MemoryStore::default();
        let private_key = PrivateKey::new();
        let body = Body::new("hello!".as_bytes());

        let (hash, header, header_bytes) = create_operation(&private_key, &body, 0, 0, None);

        // Insert one operation.
        let inserted = store
            .insert_operation(hash, &header, Some(&body), &header_bytes, &0)
            .await
            .expect("no errors");
        assert!(inserted);

        // We expect one log and one operation.
        assert_eq!(store.read_store().logs.len(), 1);
        assert_eq!(store.read_store().operations.len(), 1);

        // Delete the operation.
        assert!(store.delete_operation(hash).await.expect("no error"));

        // We expect no logs and no operations.
        assert_eq!(store.read_store().logs.len(), 0);
        assert_eq!(store.read_store().operations.len(), 0);

        let deleted_operation = store.get_operation(hash).await.expect("no error");
        assert!(deleted_operation.is_none());
        assert!(!store.has_operation(hash).await.expect("no error"));

        let deleted_raw_operation = store.get_raw_operation(hash).await.expect("no error");
        assert!(deleted_raw_operation.is_none());
    }

    #[tokio::test]
    async fn delete_payload() {
        let mut store = MemoryStore::default();
        let private_key = PrivateKey::new();
        let body = Body::new("hello!".as_bytes());

        let (hash, header, header_bytes) = create_operation(&private_key, &body, 0, 0, None);

        let inserted = store
            .insert_operation(hash, &header, Some(&body), &header_bytes, &0)
            .await
            .expect("no errors");
        assert!(inserted);

        assert!(store.delete_payload(hash).await.expect("no error"));

        let (_, no_body) = store
            .get_operation(hash)
            .await
            .expect("no error")
            .expect("operation exist");
        assert!(no_body.is_none());
        assert!(store.has_operation(hash).await.expect("no error"));

        let (_, no_body) = store
            .get_raw_operation(hash)
            .await
            .expect("no error")
            .expect("operation exist");
        assert!(no_body.is_none());
    }

    #[tokio::test]
    async fn get_log() {
        let mut store = MemoryStore::default();
        let private_key = PrivateKey::new();
        let log_id = 0;

        let body_0 = Body::new("hello!".as_bytes());
        let body_1 = Body::new("hello again!".as_bytes());
        let body_2 = Body::new("hello for a third time!".as_bytes());

        let (hash_0, header_0, header_bytes_0) =
            create_operation(&private_key, &body_0, 0, 0, None);
        let (hash_1, header_1, header_bytes_1) =
            create_operation(&private_key, &body_1, 1, 0, Some(hash_0));
        let (hash_2, header_2, header_bytes_2) =
            create_operation(&private_key, &body_2, 2, 0, Some(hash_1));

        store
            .insert_operation(hash_0, &header_0, Some(&body_0), &header_bytes_0, &0)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_1, &header_1, Some(&body_1), &header_bytes_1, &0)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_2, &header_2, Some(&body_2), &header_bytes_2, &0)
            .await
            .expect("no errors");

        // Get all log operations.
        let log = store
            .get_log(&private_key.public_key(), &log_id, None)
            .await
            .expect("no errors")
            .expect("log should exist");

        assert_eq!(log.len(), 3);
        assert_eq!(log[0].0.hash(), hash_0);
        assert_eq!(log[1].0.hash(), hash_1);
        assert_eq!(log[2].0.hash(), hash_2);
        assert_eq!(log[0].1, Some(body_0.clone()));
        assert_eq!(log[1].1, Some(body_1.clone()));
        assert_eq!(log[2].1, Some(body_2.clone()));

        // Get all log operations starting from sequence number 1.
        let log = store
            .get_log(&private_key.public_key(), &log_id, Some(1))
            .await
            .expect("no errors")
            .expect("log should exist");

        assert_eq!(log.len(), 2);
        assert_eq!(log[0].0.hash(), hash_1);
        assert_eq!(log[1].0.hash(), hash_2);
        assert_eq!(log[0].1, Some(body_1.clone()));
        assert_eq!(log[1].1, Some(body_2.clone()));

        // Get all raw log operations.
        let log = store
            .get_raw_log(&private_key.public_key(), &log_id, None)
            .await
            .expect("no errors")
            .expect("log should exist");

        assert_eq!(log.len(), 3);
        assert_eq!(log[0].0, header_bytes_0);
        assert_eq!(log[1].0, header_bytes_1);
        assert_eq!(log[2].0, header_bytes_2);
        assert_eq!(log[0].1, Some(body_0.to_bytes()));
        assert_eq!(log[1].1, Some(body_1.to_bytes()));
        assert_eq!(log[2].1, Some(body_2.to_bytes()));

        // Get all raw log operations starting from sequence number 1.
        let log = store
            .get_raw_log(&private_key.public_key(), &log_id, Some(1))
            .await
            .expect("no errors")
            .expect("log should exist");

        assert_eq!(log.len(), 2);
        assert_eq!(log[0].0, header_bytes_1);
        assert_eq!(log[1].0, header_bytes_2);
        assert_eq!(log[0].1, Some(body_1.to_bytes()));
        assert_eq!(log[1].1, Some(body_2.to_bytes()));
    }

    #[tokio::test]
    async fn insert_many_get_one_log() {
        let mut store = MemoryStore::default();
        let private_key = PrivateKey::new();
        let log_a_id = "a";
        let log_b_id = "b";

        let body_a0 = Body::new("hello from log a!".as_bytes());
        let body_a1 = Body::new("hello from log a again!".as_bytes());
        let (hash_a0, header_a0, header_bytes_a0) =
            create_operation(&private_key, &body_a0, 0, 0, None);
        let (hash_a1, header_a1, header_bytes_a1) =
            create_operation(&private_key, &body_a1, 1, 1, Some(hash_a0));

        let inserted = store
            .insert_operation(
                hash_a0,
                &header_a0,
                Some(&body_a0),
                &header_bytes_a0,
                &log_a_id,
            )
            .await
            .expect("no errors");
        assert!(inserted);

        let inserted = store
            .insert_operation(
                hash_a1,
                &header_a1,
                Some(&body_a1),
                &header_bytes_a1,
                &log_a_id,
            )
            .await
            .expect("no errors");
        assert!(inserted);

        let body_b0 = Body::new("hello from log b!".as_bytes());
        let body_b1 = Body::new("hello from log b again!".as_bytes());
        let (hash_b0, header_b0, header_bytes_b0) =
            create_operation(&private_key, &body_b0, 0, 3, None);
        let (hash_b1, header_b1, header_bytes_b1) =
            create_operation(&private_key, &body_b1, 1, 4, Some(hash_b0));

        store
            .insert_operation(
                hash_b0,
                &header_b0,
                Some(&body_b0),
                &header_bytes_b0,
                &log_b_id,
            )
            .await
            .expect("no errors");

        store
            .insert_operation(
                hash_b1,
                &header_b1,
                Some(&body_b1),
                &header_bytes_b1,
                &log_b_id,
            )
            .await
            .expect("no errors");

        let log_a = store
            .get_log(&private_key.public_key(), &log_a_id, None)
            .await
            .expect("no errors")
            .expect("log should exist");

        assert_eq!(log_a.len(), 2);
        assert_eq!(log_a[0].0.hash(), header_a0.hash());
        assert_eq!(log_a[1].0.hash(), header_a1.hash());

        let log_b = store
            .get_log(&private_key.public_key(), &log_b_id, None)
            .await
            .expect("no errors")
            .expect("log should exist");

        assert_eq!(log_b.len(), 2);
        assert_eq!(log_b[0].0.hash(), header_b0.hash());
        assert_eq!(log_b[1].0.hash(), header_b1.hash());
    }

    #[tokio::test]
    async fn many_authors_same_log_id() {
        let mut store = MemoryStore::default();
        let private_key_a = PrivateKey::new();
        let private_key_b = PrivateKey::new();
        let log_id = 0;
        let body = Body::new("hello!".as_bytes());

        let (hash_a, header_a, header_bytes_a) =
            create_operation(&private_key_a, &body, 0, 0, None);
        let inserted = store
            .insert_operation(hash_a, &header_a, Some(&body), &header_bytes_a, &log_id)
            .await
            .expect("no errors");
        assert!(inserted);

        let (hash_b, header_b, header_bytes_b) =
            create_operation(&private_key_b, &body, 0, 0, None);
        let inserted = store
            .insert_operation(hash_b, &header_b, Some(&body), &header_bytes_b, &log_id)
            .await
            .expect("no errors");
        assert!(inserted);

        let author_a_log = store
            .get_log(&private_key_a.public_key(), &log_id, None)
            .await
            .expect("no errors")
            .expect("log should exist");

        assert_eq!(author_a_log.len(), 1);
        assert_eq!(author_a_log[0].0.hash(), header_a.hash());

        let author_b_log = store
            .get_log(&private_key_b.public_key(), &log_id, None)
            .await
            .expect("no errors")
            .expect("log should exist");

        assert_eq!(author_b_log.len(), 1);
        assert_eq!(author_b_log[0].0.hash(), header_b.hash());
    }

    #[tokio::test]
    async fn get_latest_operation() {
        let mut store = MemoryStore::default();
        let private_key = PrivateKey::new();
        let log_id = 0;

        let body_0 = Body::new("hello!".as_bytes());
        let body_1 = Body::new("hello again!".as_bytes());

        let (hash_0, header_0, header_bytes_0) =
            create_operation(&private_key, &body_0, 0, 0, None);
        let (hash_1, header_1, header_bytes_1) =
            create_operation(&private_key, &body_1, 1, 0, Some(hash_0));

        store
            .insert_operation(hash_0, &header_0, Some(&body_0), &header_bytes_0, &log_id)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_1, &header_1, Some(&body_1), &header_bytes_1, &log_id)
            .await
            .expect("no errors");

        let (latest_header, latest_body) = store
            .latest_operation(&private_key.public_key(), &log_id)
            .await
            .expect("no errors")
            .expect("there's an operation");

        assert_eq!(latest_header.hash(), header_1.hash());
        assert_eq!(latest_body, Some(body_1));
    }

    #[tokio::test]
    async fn delete_operations() {
        let mut store = MemoryStore::default();
        let private_key = PrivateKey::new();
        let log_id = 0;

        let body_0 = Body::new("hello!".as_bytes());
        let body_1 = Body::new("hello again!".as_bytes());
        let body_2 = Body::new("final hello!".as_bytes());

        let (hash_0, header_0, header_bytes_0) =
            create_operation(&private_key, &body_0, 0, 0, None);
        let (hash_1, header_1, header_bytes_1) =
            create_operation(&private_key, &body_1, 1, 100, Some(hash_0));
        let (hash_2, header_2, header_bytes_2) =
            create_operation(&private_key, &body_2, 2, 200, Some(hash_1));

        store
            .insert_operation(hash_0, &header_0, Some(&body_0), &header_bytes_0, &log_id)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_1, &header_1, Some(&body_1), &header_bytes_1, &log_id)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_2, &header_2, Some(&body_2), &header_bytes_2, &log_id)
            .await
            .expect("no errors");

        // We expect one log and 3 operations.
        assert_eq!(store.read_store().logs.len(), 1);
        assert_eq!(store.read_store().operations.len(), 3);

        // Delete all operations _before_ seq_num 2.
        let deleted = store
            .delete_operations(&private_key.public_key(), &log_id, 2)
            .await
            .expect("no errors");
        assert!(deleted);

        // There is now only one operation in the log.
        assert_eq!(store.read_store().logs.len(), 1);
        assert_eq!(store.read_store().operations.len(), 1);

        // The remaining operation in the log should be the latest (seq_num == 2).
        let log = store
            .get_log(&private_key.public_key(), &log_id, None)
            .await
            .expect("no errors")
            .expect("log should exist");
        assert_eq!(log[0].0.hash(), header_2.hash());

        // Deleting the same range again should return `false`, meaning no deletion occurred.
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

        let body_0 = Body::new("hello!".as_bytes());
        let body_1 = Body::new("hello again!".as_bytes());
        let body_2 = Body::new("final hello!".as_bytes());

        let (hash_0, header_0, header_bytes_0) =
            create_operation(&private_key, &body_0, 0, 0, None);
        let (hash_1, header_1, header_bytes_1) =
            create_operation(&private_key, &body_1, 1, 100, Some(hash_0));
        let (hash_2, header_2, header_bytes_2) =
            create_operation(&private_key, &body_2, 2, 200, Some(hash_1));

        store
            .insert_operation(hash_0, &header_0, Some(&body_0), &header_bytes_0, &log_id)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_1, &header_1, Some(&body_1), &header_bytes_1, &log_id)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_2, &header_2, Some(&body_2), &header_bytes_2, &log_id)
            .await
            .expect("no errors");

        // There is one log and 3 operations.
        assert_eq!(store.read_store().logs.len(), 1);
        assert_eq!(store.read_store().operations.len(), 3);

        // Delete all operation payloads from sequence number 0 up to but not including 2.
        let deleted = store
            .delete_payloads(&private_key.public_key(), &log_id, 0, 2)
            .await
            .expect("no errors");
        assert!(deleted);

        let log = store
            .get_log(&private_key.public_key(), &log_id, None)
            .await
            .expect("no errors")
            .expect("log should exist");

        assert_eq!(log[0].1, None);
        assert_eq!(log[1].1, None);
        assert_eq!(log[2].1, Some(body_2));
    }
}
