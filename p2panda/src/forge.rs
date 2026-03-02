// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error as StdError;
use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

use p2panda_core::{Body, Hash, PrivateKey, PublicKey, SeqNum, Topic};
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore, tx};
use thiserror::Error;

use crate::operation::{Extensions, Header, Operation};

/// Interface for obtaining a keypair and creating signed operations.
pub trait Forge<T, C, E> {
    type Error: StdError;

    fn private_key(&self) -> &PrivateKey;

    fn public_key(&self) -> PublicKey;

    fn create_operation(
        &mut self,
        topic: T,
        collection_id: C,
        body: Option<Vec<u8>>,
        extensions: E,
    ) -> impl Future<Output = Result<Option<p2panda_core::Operation<E>>, Self::Error>>;
}

#[derive(Clone, Debug)]
pub struct OperationForge {
    private_key: PrivateKey,
    store: SqliteStore<'static>,
}

impl OperationForge {
    /// Create a forge for inserting signed operations into the database and associating topics
    /// with logs.
    ///
    /// The forge holds the private key used to sign operations. This method generates a new key
    /// using CSPRNG from the system.
    pub fn new(store: SqliteStore<'static>) -> Self {
        Self {
            private_key: PrivateKey::new(),
            store,
        }
    }

    /// Create a forge using an existing private key.
    pub fn from_private_key(private_key: PrivateKey, store: SqliteStore<'static>) -> Self {
        Self { private_key, store }
    }
}

type LogId = [u8; 32];

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
        &mut self,
        topic: Topic,
        log_id: LogId,
        body: Option<Vec<u8>>,
        extensions: Extensions,
    ) -> Result<Option<Operation>, Self::Error> {
        let (seq_num, backlink) = <SqliteStore<'static> as LogStore<
            Operation,
            PublicKey,
            [u8; 32],
            SeqNum,
            Hash,
        >>::get_latest_entry(
            &self.store, &self.private_key.public_key(), &log_id
        )
        .await?
        .map(|(hash, seq_num)| (seq_num + 1, Some(hash)))
        .unwrap_or((0, None));

        let body: Option<Body> = body.map(|bytes| bytes.into());

        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        let mut header = Header {
            version: 1,
            public_key: self.private_key.public_key(),
            signature: None,
            payload_size: body.as_ref().map(|body| body.size()).unwrap_or(0),
            payload_hash: body.as_ref().map(|body| body.hash()),
            timestamp,
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

        // Acquire a store permit, associate the topic with the log, insert the
        // operation and commit the transaction.
        let inserted = tx!(self.store, {
            <SqliteStore<'static> as TopicStore<Topic, PublicKey, [u8; 32]>>::associate(
                &self.store,
                &topic,
                &self.private_key.public_key(),
                &log_id,
            )
            .await?;

            self.store
                .insert_operation(&hash.clone(), operation.clone(), log_id)
                .await?
        });

        Ok(inserted.then_some(operation))
    }
}

#[derive(Debug, Error)]
pub enum ForgeError {
    #[error(transparent)]
    Sqlite(#[from] SqliteError),

    #[error(transparent)]
    SystemTime(#[from] SystemTimeError),
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use p2panda_core::{Body, Operation, Topic};
    use p2panda_store::SqliteStore;
    use p2panda_store::logs::LogStore;

    use crate::Extensions;
    use crate::forge::Forge;

    use super::OperationForge;

    #[tokio::test]
    async fn operation_forge() {
        let store = SqliteStore::temporary().await;
        let mut forge = OperationForge::new(store.clone());

        let topic = Topic::new();
        let log_id: [u8; 32] = Topic::new().into();
        let extensions = Extensions { version: 1 };

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

        let public_key = forge.public_key();

        let result = <SqliteStore<'_> as LogStore<Operation, _, _, _, _>>::get_log_heights(
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
