// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::sync::Arc;

use p2panda_core::{Hash, SigningKey, VerifyingKey};
use tokio::sync::RwLock;

use crate::message::{SpacesArgs, SpacesMessage};
use crate::test_utils::message::SeqNum;
use crate::test_utils::{TestConditions, TestSpaceId};
use crate::traits::{Forge, MessageStore};

#[derive(Debug, Clone)]
pub struct TestForge<S> {
    verifying_key: VerifyingKey,
    store: S,
    inner: Arc<RwLock<TestForgeInner>>,
}

impl<S> TestForge<S>
where
    S: MessageStore<SpacesMessage<TestSpaceId, TestConditions>>,
{
    pub fn new(store: S, signing_key: SigningKey) -> Self {
        Self {
            verifying_key: signing_key.verifying_key(),
            store,
            inner: Arc::new(RwLock::new(TestForgeInner {
                next_seq_num: 0,
                signing_key,
            })),
        }
    }
}

#[derive(Debug)]
pub struct TestForgeInner {
    #[allow(unused)]
    next_seq_num: SeqNum,
    #[allow(unused)]
    signing_key: SigningKey,
}

impl<S> Forge<TestSpaceId, TestConditions> for TestForge<S>
where
    S: MessageStore<SpacesMessage<TestSpaceId, TestConditions>>,
{
    type Message = SpacesMessage<TestSpaceId, TestConditions>;
    type Error = Infallible;

    fn verifying_key(&self) -> VerifyingKey {
        self.verifying_key
    }

    async fn forge(
        &self,
        args: SpacesArgs<TestSpaceId, TestConditions>,
    ) -> Result<Self::Message, Self::Error> {
        let seq_num = {
            let mut inner = self.inner.write().await;
            let seq_num = inner.next_seq_num;
            inner.next_seq_num += 1;
            seq_num
        };

        let mut buffer: Vec<u8> = self.verifying_key.as_bytes().to_vec();
        buffer.extend_from_slice(&seq_num.to_be_bytes());
        let hash = Hash::digest(buffer).into();

        let message = SpacesMessage {
            id: hash,
            author: self.verifying_key().into(),
            args,
        };

        self.store.set_message(&message.id, &message).await.unwrap();

        Ok(message)
    }
}
