// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error as StdError;
use std::sync::Arc;

use p2panda_core::{Body, Hash, PrivateKey, PublicKey, SeqNum, Timestamp, Topic};
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore, tx};
use thiserror::Error;

use crate::operation::{Extensions, Header, LogId, Operation};

/// Interface for obtaining a keypair and creating signed operations.
pub trait Forge<TP, C, E> {
    type Error: StdError;

    fn private_key(&self) -> &PrivateKey;

    fn public_key(&self) -> PublicKey;

    fn create_operation(
        &self,
        topic: TP,
        collection_id: C,
        body: Option<Vec<u8>>,
        extensions: E,
    ) -> impl Future<Output = Result<Option<p2panda_core::Operation<E>>, Self::Error>>;
}

#[derive(Clone, Debug)]
pub struct OperationForge {
    private_key: Arc<PrivateKey>,
    store: SqliteStore,
}

impl OperationForge {
    /// Create a forge for inserting signed operations into the database and associating topics
    /// with logs.
    ///
    /// The forge holds the private key used to sign operations. This method generates a new key
    /// using CSPRNG from the system.
    pub fn new(store: SqliteStore) -> Self {
        Self::from_private_key(PrivateKey::new(), store)
    }

    /// Create a forge using an existing private key.
    pub fn from_private_key(private_key: PrivateKey, store: SqliteStore) -> Self {
        Self {
            private_key: Arc::new(private_key),
            store,
        }
    }
}

impl Forge<Topic, LogId, Extensions> for OperationForge {
    type Error = ForgeError;

    fn private_key(&self) -> &PrivateKey {
        &self.private_key
    }

    fn public_key(&self) -> PublicKey {
        self.private_key.public_key()
    }

    /// Create a signed operation and insert it into the store.
    ///
    /// This method performs several actions: it first queries the store to determine the latest
    /// entry for the given author and log id. It then composes an operation and signs it. Finally,
    /// the relevant log is associated with the topic and the signed operation is inserted into the
    /// store. Both the log-topic association and operation insertion are executed as part of a
    /// single transaction, thereby ensuring atomicity.
    async fn create_operation(
        &self,
        topic: Topic,
        log_id: LogId,
        body: Option<Vec<u8>>,
        extensions: Extensions,
    ) -> Result<Option<Operation>, Self::Error> {
        // Perform prerequisite computations outside of the locked transaction.
        let payload_size = body.as_ref().map(|bytes| bytes.len()).unwrap_or(0) as u64;
        let body: Option<Body> = body.map(|bytes| bytes.into());
        let payload_hash = body.as_ref().map(|body| body.hash());

        // Acquire a lock on the store for the duration of the read to write cycle.
        //
        // This is to ensure that the data returned from the `get_latest_entry()` query does not
        // become stale before the call to `insert_operation()`.
        //
        // Here we acquire a store permit, query the latest log entry, associate the topic with
        // the log, insert the operation and commit the transaction before dropping the permit.
        let operation = tx!(self.store, {
            let (seq_num, backlink) = <SqliteStore as LogStore<
                Operation,
                PublicKey,
                LogId,
                SeqNum,
                Hash,
            >>::get_latest_entry_tx(
                &self.store, &self.private_key.public_key(), &log_id
            )
            .await?
            .map(|operation| (operation.header.seq_num + 1, Some(operation.hash)))
            .unwrap_or((0, None));

            let mut header = Header {
                version: 1,
                public_key: self.private_key.public_key(),
                signature: None,
                payload_size,
                payload_hash,
                timestamp: Timestamp::now(),
                seq_num,
                backlink,
                extensions,
            };

            header.sign(&self.private_key);
            let hash = header.hash();

            let operation = Operation {
                hash,
                header: header.clone(),
                body,
            };

            <SqliteStore as TopicStore<Topic, PublicKey, LogId>>::associate(
                &self.store,
                &topic,
                &self.private_key.public_key(),
                &log_id,
            )
            .await?;

            self.store
                .insert_operation(&hash, &operation, &log_id)
                .await?
                .then_some(operation)
        });

        Ok(operation)
    }
}

#[derive(Debug, Error)]
pub enum ForgeError {
    #[error(transparent)]
    Sqlite(#[from] SqliteError),
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use p2panda_core::{Operation, Topic};
    use p2panda_store::SqliteStore;
    use p2panda_store::logs::LogStore;

    use crate::forge::Forge;
    use crate::operation::{Extensions, LogId};

    use super::OperationForge;

    #[tokio::test]
    async fn operation_forge() {
        let store = SqliteStore::temporary().await;
        let forge = OperationForge::new(store.clone());

        let topic = Topic::new();
        let log_id = LogId::from_topic(topic);
        let extensions = Extensions::from_topic(topic);

        forge
            .create_operation(
                topic,
                log_id,
                Some("spring!".as_bytes().to_vec()),
                extensions.clone(),
            )
            .await
            .unwrap()
            .unwrap();

        forge
            .create_operation(
                topic,
                log_id,
                Some("summer!".as_bytes().to_vec()),
                extensions,
            )
            .await
            .unwrap()
            .unwrap();

        let result = <SqliteStore as LogStore<Operation, _, _, _, _>>::get_log_heights(
            &store,
            &forge.public_key(),
            &[log_id],
        )
        .await
        .unwrap();

        let expected_result = BTreeMap::from([(log_id, 1)]);

        assert_eq!(result, Some(expected_result));
    }
}
