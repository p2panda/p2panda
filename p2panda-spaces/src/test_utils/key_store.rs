// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{convert::Infallible, marker::PhantomData};

use p2panda_core::{PrivateKey, PublicKey};
use p2panda_encryption::Rng;
use p2panda_encryption::key_manager::{KeyManager, KeyManagerError, KeyManagerState};
use p2panda_encryption::key_registry::{KeyRegistry, KeyRegistryState};

use crate::{Config, Credentials};
use crate::message::SpacesArgs;
use crate::test_utils::{SeqNum, TestConditions, TestMessage};
use crate::traits::SpaceId;
use crate::traits::key_store::{Forge, KeyStore};
use crate::types::ActorId;

#[derive(Debug)]
pub struct TestKeyStore<ID> {
    next_seq_num: SeqNum,
    private_key: PrivateKey,
    key_manager: KeyManagerState,
    key_registry: KeyRegistryState<ActorId>,
    _phantom: PhantomData<ID>,
}

impl<ID> TestKeyStore<ID> {
    pub fn new(
        credentials: &Credentials,
        config: &Config,
        rng: &Rng,
    ) -> Result<Self, KeyManagerError> {
        let key_manager = KeyManager::init(&credentials.identity_secret(), config.lifetime(), rng)?;
        let key_registry = KeyRegistry::init();
        Ok(Self {
            next_seq_num: 0,
            private_key: credentials.private_key(),
            key_manager,
            key_registry,
            _phantom: PhantomData,
        })
    }
}

impl<ID> KeyStore for TestKeyStore<ID>
where
    ID: SpaceId,
{
    type Error = Infallible;

    async fn key_manager(&self) -> Result<KeyManagerState, Self::Error> {
        Ok(self.key_manager.clone())
    }

    async fn key_registry(&self) -> Result<KeyRegistryState<ActorId>, Self::Error> {
        Ok(self.key_registry.clone())
    }

    async fn set_key_manager(&mut self, y: &KeyManagerState) -> Result<(), Self::Error> {
        self.key_manager = y.clone();
        Ok(())
    }

    async fn set_key_registry(&mut self, y: &KeyRegistryState<ActorId>) -> Result<(), Self::Error> {
        self.key_registry = y.clone();
        Ok(())
    }
}

impl<ID> Forge<ID, TestMessage<ID>, TestConditions> for TestKeyStore<ID>
where
    ID: SpaceId,
{
    type Error = Infallible;

    fn public_key(&self) -> PublicKey {
        self.private_key.public_key()
    }

    async fn forge(
        &mut self,
        args: SpacesArgs<ID, TestConditions>,
    ) -> Result<TestMessage<ID>, Self::Error> {
        let seq_num = self.next_seq_num;
        self.next_seq_num += 1;
        Ok(TestMessage {
            seq_num,
            public_key: self.public_key(),
            spaces_args: args,
        })
    }

    async fn forge_ephemeral(
        &mut self,
        private_key: PrivateKey,
        args: SpacesArgs<ID, TestConditions>,
    ) -> Result<TestMessage<ID>, Self::Error> {
        Ok(TestMessage {
            // Will always be first entry in the "log" as we're dropping the private key.
            seq_num: 0,
            public_key: private_key.public_key(),
            spaces_args: args,
        })
    }
}
