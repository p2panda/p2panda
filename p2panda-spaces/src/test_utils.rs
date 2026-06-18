// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;

use p2panda_core::{Hash, Header, SeqNum, SigningKey, VerifyingKey};
use p2panda_encryption::Rng;
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::spaces::SqliteSpacesStore;
use p2panda_store::{SqliteError, SqliteStore, tx};

use crate::forge::Forge;
use crate::manager::Manager;
use crate::space::SpaceError;
use crate::types::StrongRemoveResolver;
use crate::{Config, Credentials, SpacesArgs};

pub type TestExtensions = SpacesArgs<TestConditions>;

pub type TestOperation = p2panda_core::Operation<TestExtensions>;

impl Borrow<SpacesArgs<TestConditions>> for TestOperation {
    fn borrow(&self) -> &SpacesArgs<TestConditions> {
        &self.header.extensions
    }
}

pub type TestPeerId = u8;

pub type TestConditions = ();

pub type TestSpacesStore = p2panda_store::spaces::SqliteSpacesStore<TestExtensions>;

pub type TestManager = Manager<
    SqliteSpacesStore<TestExtensions>,
    TestForge,
    TestConditions,
    StrongRemoveResolver<TestConditions>,
>;

pub type TestSpaceError =
    SpaceError<TestForge, TestConditions, StrongRemoveResolver<TestConditions>>;

#[allow(unused)]
pub struct TestPeer {
    pub(crate) id: TestPeerId,
    pub(crate) manager: TestManager,
    pub(crate) credentials: Credentials,
}

impl TestPeer {
    pub async fn new(peer_id: TestPeerId) -> Self {
        let rng = Rng::from_seed([peer_id; 32]);
        let credentials = Credentials::from_rng(&rng).unwrap();
        let config = Config::default();
        Self::new_with_config(peer_id, credentials, config, rng).await
    }

    pub async fn new_with_config(
        peer_id: TestPeerId,
        credentials: Credentials,
        config: Config,
        rng: Rng,
    ) -> Self {
        let store = SqliteStore::temporary().await;
        let spaces_store = TestSpacesStore::new(store.clone());
        let forge = TestForge::new(store, credentials.signing_key());

        let manager =
            TestManager::new_with_config(spaces_store, forge, credentials.clone(), config, rng)
                .await
                .unwrap();

        Self {
            id: peer_id,
            manager,
            credentials,
        }
    }
}

type LogId = u32;

// Write all spaces operations into one single log.
pub const DEFAULT_LOG_ID: LogId = 0;

#[derive(Debug, Clone)]
pub struct TestForge {
    signing_key: SigningKey,
    store: SqliteStore,
}

impl TestForge {
    pub fn new(store: SqliteStore, signing_key: SigningKey) -> Self {
        Self { signing_key, store }
    }
}

impl Forge<TestConditions> for TestForge {
    type Message = TestOperation;

    type Error = SqliteError;

    fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    async fn forge(&self, args: SpacesArgs<TestConditions>) -> Result<Self::Message, Self::Error> {
        let operation = tx!(self.store, {
            let (seq_num, backlink) = <SqliteStore as LogStore<
                TestOperation,
                VerifyingKey,
                LogId,
                SeqNum,
                Hash,
            >>::get_latest_entry_tx(
                &self.store,
                &self.signing_key.verifying_key(),
                &DEFAULT_LOG_ID,
            )
            .await?
            .map(|operation| (operation.header.seq_num + 1, Some(operation.hash)))
            .unwrap_or((0, None));

            let mut header = Header {
                version: 1,
                verifying_key: self.signing_key.verifying_key(),
                signature: None,
                payload_size: 0,
                payload_hash: None,
                seq_num,
                backlink,
                extensions: args,
            };

            header.sign(&self.signing_key);
            let hash = header.hash();

            let operation = TestOperation {
                hash,
                header: header.clone(),
                body: None,
            };

            self.store
                .insert_operation(&hash, &operation, &DEFAULT_LOG_ID)
                .await?;

            operation
        });

        Ok(operation)
    }
}
