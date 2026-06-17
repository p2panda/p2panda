// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{Hash, Header, SigningKey, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::{SqliteStore, tx};

use crate::message::SpacesArgs;
use crate::test_utils::{Operation, SpacesExtensions, TestConditions, TestSpaceId};
use crate::traits::Forge;

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

impl Forge<TestSpaceId, TestConditions> for TestForge {
    type Message = Operation;
    type Error = anyhow::Error;

    fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    async fn forge(
        &self,
        args: SpacesArgs<TestSpaceId, TestConditions>,
    ) -> Result<Self::Message, Self::Error> {
        // Perform prerequisite computations outside of the locked transaction.
        let payload_size = 0;
        let body = None;
        let payload_hash = None;
        let extensions = SpacesExtensions { args: args.clone() };

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
                payload_size,
                payload_hash,
                seq_num,
                backlink,
                extensions,
            };

            header.sign(&self.signing_key);
            let hash = header.hash();

            let operation = Operation {
                hash,
                header: header.clone(),
                body,
            };

            self.store
                .insert_operation(&hash, &operation, &DEFAULT_LOG_ID)
                .await?;

            operation
        });

        Ok(operation)
    }
}
