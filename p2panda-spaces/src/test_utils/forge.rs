// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{Hash, Header, SigningKey, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::{SqliteError, SqliteStore, tx};

use crate::forge::Forge;
use crate::message::SpacesArgs;
use crate::test_utils::{TestConditions, TestOperation};

pub const DEFAULT_LOG_ID: u32 = 0;

type LogId = u32;
type SeqNum = u32;

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
                header,
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
