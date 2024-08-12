// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeSet, HashMap};

use p2panda_core::{Extension, Hash, Operation, PublicKey};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::traits::{OperationStore, StoreError};
use crate::{LogId, LogStore};

type SeqNum = u64;
type Timestamp = u64;
type LogMeta = (SeqNum, Timestamp, Hash);

#[derive(Debug, Default)]
pub struct MemoryStore<E> {
    operations: HashMap<Hash, Operation<E>>,
    logs: HashMap<(PublicKey, LogId), BTreeSet<LogMeta>>,
}

impl<E> MemoryStore<E>
where
    E: Clone + Extension<LogId>,
{
    pub fn new() -> Self {
        Self {
            operations: Default::default(),
            logs: Default::default(),
        }
    }
}

impl<E> OperationStore<E> for MemoryStore<E>
where
    E: Clone + Extension<LogId>,
{
    type LogId = LogId;

    fn insert_operation(&mut self, operation: Operation<E>) -> Result<bool, StoreError> {
        let entry = (
            operation.header.seq_num,
            operation.header.timestamp,
            operation.hash,
        );

        let log_id = Extension::<Self::LogId>::extract(&operation.header)
            .unwrap_or(LogId::from_public_key(operation.header.public_key));

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
            let log_id = Extension::<Self::LogId>::extract(&operation.header)
                .unwrap_or(LogId::from_public_key(operation.header.public_key));

            self.logs
                .get_mut(&(operation.header.public_key, log_id))
                .unwrap()
                .remove(&(
                    operation.header.seq_num,
                    operation.header.timestamp,
                    operation.hash,
                ));
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

impl<E> LogStore<E> for MemoryStore<E>
where
    E: Clone + Serialize + DeserializeOwned + Extension<LogId>,
{
    type LogId = LogId;

    fn get_log(
        &self,
        public_key: PublicKey,
        log_id: LogId,
    ) -> Result<Option<Vec<Operation<E>>>, StoreError> {
        todo!()
    }

    fn latest_operation(
        &self,
        public_key: PublicKey,
        log_id: LogId,
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
        log_id: LogId,
        from: u64,
        to: Option<u64>,
    ) -> Result<bool, StoreError> {
        todo!()
    }

    fn delete_payloads(
        &mut self,
        public_key: PublicKey,
        log_id: LogId,
        from: u64,
        to: Option<u64>,
    ) -> Result<bool, StoreError> {
        todo!()
    }
}
#[cfg(test)]
mod tests {
    use p2panda_core::{validate_operation, Body, Extension, Header, Operation, PrivateKey};
    use serde::{Deserialize, Serialize};

    use crate::traits::OperationStore;

    use super::{LogId, MemoryStore};

    #[derive(Clone, Deserialize, Serialize)]
    pub struct MyCustomExtensions {
        log_id: LogId,
    }

    impl Extension<LogId> for MyCustomExtensions {
        fn extract(&self) -> Option<LogId> {
            Some(self.log_id.clone())
        }
    }

    #[test]
    fn generic_extensions_mem_store_support() {
        let private_key = PrivateKey::new();
        let body = Body::new("hello!".as_bytes());
        let extensions = MyCustomExtensions {
            log_id: "messages".to_string().into(),
        };

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
            extensions: Some(extensions),
        };
        header.sign(&private_key);

        let operation = Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        };
        assert!(validate_operation(&operation).is_ok());

        let mut my_store = MemoryStore::new();
        assert_eq!(my_store.insert_operation(operation).ok(), Some(true));
    }
}
