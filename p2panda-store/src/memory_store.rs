// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeSet, HashMap};

use p2panda_core::extensions::DefaultExtensions;
use p2panda_core::{Hash, Operation, PublicKey};

use crate::traits::{OperationStore, StoreError};
use crate::LogStore;

type SeqNum = u64;
type Timestamp = u64;
type LogMeta = (SeqNum, Timestamp, Hash);

#[derive(Debug)]
pub struct MemoryStore<T, E> {
    operations: HashMap<Hash, Operation<E>>,
    logs: HashMap<(PublicKey, T), BTreeSet<LogMeta>>,
}

impl<T, E> MemoryStore<T, E> {
    pub fn new() -> Self {
        Self {
            operations: Default::default(),
            logs: Default::default(),
        }
    }
}

impl<T> Default for MemoryStore<T, DefaultExtensions> {
    fn default() -> Self {
        Self {
            operations: Default::default(),
            logs: Default::default(),
        }
    }
}

impl<T, E> OperationStore<T, E> for MemoryStore<T, E>
where
    T: Eq + std::hash::Hash + Default,
    E: Clone,
{
    fn insert_operation(&mut self, operation: Operation<E>, log_id: T) -> Result<bool, StoreError> {
        let entry = (
            operation.header.seq_num,
            operation.header.timestamp,
            operation.hash,
        );

        self.logs
            .entry((operation.header.public_key, log_id))
            .and_modify(|log| {
                log.insert(entry);
            })
            .or_insert(BTreeSet::from([entry]));
        self.operations.insert(operation.hash, operation);
        Ok(true)
    }

    fn get_operation(&self, hash: Hash) -> Result<Option<Operation<E>>, StoreError> {
        Ok(self.operations.get(&hash).cloned())
    }

    fn delete_operation(&mut self, hash: Hash) -> Result<bool, StoreError> {
        if let Some(operation) = self.operations.remove(&hash) {
            let mut removed = false;
            self.logs.iter_mut().for_each(|(_, log)| {
                removed = log.remove(&(
                    operation.header.seq_num,
                    operation.header.timestamp,
                    operation.hash,
                ));
                if removed {
                    return;
                }
            });
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn delete_payload(&mut self, hash: Hash) -> Result<bool, StoreError> {
        if let Some(operation) = self.operations.get_mut(&hash) {
            operation.body = None;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl<T, E> LogStore<T, E> for MemoryStore<T, E>
where
    T: Eq + std::hash::Hash + Default,
    E: Clone,
{
    fn get_log(
        &self,
        public_key: PublicKey,
        log_id: T,
    ) -> Result<Option<Vec<Operation<E>>>, StoreError> {
        todo!()
    }

    fn latest_operation(
        &self,
        public_key: PublicKey,
        log_id: T,
    ) -> Result<Option<Operation<E>>, StoreError> {
        let latest = match self.logs.get(&(public_key, log_id)) {
            Some(log) => match log.last() {
                Some((_, _, hash)) => self.operations.get(&hash),
                None => None,
            },
            None => None,
        };
        Ok(latest.cloned())
    }

    fn delete_operations(
        &mut self,
        public_key: PublicKey,
        log_id: T,
        from: u64,
        to: Option<u64>,
    ) -> Result<bool, StoreError> {
        todo!()
    }

    fn delete_payloads(
        &mut self,
        public_key: PublicKey,
        log_id: T,
        from: u64,
        to: Option<u64>,
    ) -> Result<bool, StoreError> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::{validate_operation, Body, Header, Operation, PrivateKey};

    use crate::traits::OperationStore;

    use super::MemoryStore;

    #[test]
    fn default_memory_store() {
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
            extensions: None,
        };

        header.sign(&private_key);

        let operation = Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        };

        let mut memory_store = MemoryStore::default();
        assert!(memory_store.insert_operation(operation, 0).is_ok())
    }

    #[test]
    fn generic_extensions_mem_store_support() {
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
            extensions: None,
        };
        header.sign(&private_key);

        let operation = Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        };
        assert!(validate_operation(&operation).is_ok());

        let mut my_store = MemoryStore::default();
        assert_eq!(my_store.insert_operation(operation, 0).ok(), Some(true));
    }
}
