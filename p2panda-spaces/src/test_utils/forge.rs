// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::sync::Arc;

use p2panda_core::{PrivateKey, PublicKey};
use tokio::sync::RwLock;

use crate::message::SpacesArgs;
use crate::test_utils::message::SeqNum;
use crate::test_utils::{TestConditions, TestMessage, TestSpaceId};
use crate::traits::{AuthoredMessage, Forge, MessageStore};

#[derive(Debug, Clone)]
pub struct TestForge<S> {
    public_key: PublicKey,
    store: S,
    inner: Arc<RwLock<TestForgeInner>>,
}

impl<S> TestForge<S>
where
    S: MessageStore<TestMessage>,
{
    pub fn new(store: S, private_key: PrivateKey) -> Self {
        Self {
            public_key: private_key.public_key(),
            store,
            inner: Arc::new(RwLock::new(TestForgeInner {
                next_seq_num: 0,
                private_key,
            })),
        }
    }
}

#[derive(Debug)]
pub struct TestForgeInner {
    next_seq_num: SeqNum,
    private_key: PrivateKey,
}

impl<S> Forge<TestSpaceId, TestMessage, TestConditions> for TestForge<S>
where
    S: MessageStore<TestMessage>,
{
    type Error = Infallible;

    fn public_key(&self) -> PublicKey {
        self.public_key
    }

    async fn forge(
        &self,
        args: SpacesArgs<TestSpaceId, TestConditions>,
    ) -> Result<TestMessage, Self::Error> {
        let seq_num = {
            let mut inner = self.inner.write().await;
            let seq_num = inner.next_seq_num;
            inner.next_seq_num += 1;
            seq_num
        };

        let message = TestMessage {
            seq_num,
            public_key: self.public_key.clone(),
            spaces_args: args,
        };

        self.store
            .set_message(&message.id(), &message)
            .await
            .unwrap();

        Ok(message)
    }
}
