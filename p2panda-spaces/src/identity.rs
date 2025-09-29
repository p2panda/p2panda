// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;
use std::time::Duration;

use p2panda_auth::traits::Conditions;
use p2panda_encryption::key_bundle::{KeyBundleError, Lifetime, LongTermKeyBundle};
use p2panda_encryption::key_manager::{KeyManager, KeyManagerError, KeyManagerState};
use p2panda_encryption::key_registry::{KeyRegistry, KeyRegistryState};
use p2panda_encryption::traits::{IdentityManager as EncIdentityManager, KeyBundle, PreKeyManager};
use p2panda_encryption::{Rng, RngError};
use thiserror::Error;

use crate::member::Member;
use crate::message::SpacesArgs;
use crate::traits::SpaceId;
use crate::traits::key_store::{Forge, KeyStore};
use crate::traits::message::{AuthoredMessage, SpacesMessage};
use crate::types::{ActorId, OperationId};
use crate::utils::now;
use crate::{Config, Credentials};

/// Manager for functionality relating to a peers identity, holds their private key and identity
/// secret.
///
/// Exposes an api for publishing and storing/retrieving key bundles, including rotating our own
/// when they expire, as well as methods for "forging" (constructing and signing) which are signed
/// with the peers private key.
///
/// Neither of a peers keys should be rotated individually, this would result in undefined
/// behavior. Rotating both keys is possible but will result in the loss of access to existing
/// spaces.
#[derive(Debug)]
pub struct IdentityManager<ID, K, M, C> {
    key_store: K,
    credentials: Credentials,
    pre_key_lifetime: Duration,
    pre_key_rotate_after: Duration,
    my_keys_rotated_at: u64,
    rng: Rng,
    _phantom: PhantomData<(ID, K, M, C)>,
}

impl<ID, K, M, C> IdentityManager<ID, K, M, C>
where
    ID: SpaceId,
    K: KeyStore + Forge<ID, M, C> + Debug,
    M: AuthoredMessage + SpacesMessage<ID, C>,
    C: Conditions,
{
    pub async fn new(
        key_store: K,
        config: &Config,
        rng: &Rng,
    ) -> Result<Self, IdentityError<ID, K, M, C>> {
        let rng = Rng::from_seed(rng.random_array()?);
        let manager = Self {
            credentials: config.credentials().to_owned(),
            key_store,
            pre_key_lifetime: config.pre_key_lifetime.clone(),
            pre_key_rotate_after: config.pre_key_rotate_after.clone(),
            my_keys_rotated_at: 0,
            rng,
            _phantom: PhantomData,
        };
        manager.validate().await?;
        Ok(manager)
    }

    /// Validate that the credentials provided in the spaces config matches those contained in the
    /// key store. If their is a mis-match of either this indicates that key rotation has occurred
    /// unexpectedly.
    pub async fn validate(&self) -> Result<(), IdentityError<ID, K, M, C>> {
        let key_manager_y = self
            .key_store
            .key_manager()
            .await
            .map_err(IdentityError::KeyStore)?;
        let identity_secret = KeyManager::identity_secret(&key_manager_y);
        if identity_secret != &self.credentials.identity_secret() {
            return Err(IdentityError::IdentitySecretRotated);
        }
        let public_key = self.key_store.public_key();
        if public_key != self.credentials.public_key() {
            return Err(IdentityError::PrivateKeyRotated);
        }

        Ok(())
    }

    /// The public key of the local actor.
    pub(crate) fn id(&self) -> ActorId {
        self.credentials.public_key().into()
    }

    /// The local actor id and their long-term key bundle.
    ///
    /// Note: key bundle will be rotated if the latest is reaching it's configured expiry date.
    pub(crate) async fn me(&mut self) -> Result<Member, IdentityError<ID, K, M, C>> {
        let my_id = self.id();

        let key_manager_y = self
            .key_store
            .key_manager()
            .await
            .map_err(IdentityError::KeyStore)?;

        // Automatically rotate pre key when it reached critical expiry date.
        let key_bundle = if now() - self.my_keys_rotated_at > self.pre_key_rotate_after.as_secs() {
            self.my_keys_rotated_at = now();

            // This mutates the state internally.
            let key_manager_y_i = KeyManager::rotate_prekey(
                key_manager_y,
                Lifetime::new(self.pre_key_lifetime.as_secs()),
                &self.rng,
            )?;

            let key_registry_y = self
                .key_store
                .key_registry()
                .await
                .map_err(IdentityError::KeyStore)?;

            // Register our own key bundle.
            let key_bundle = KeyManager::prekey_bundle(&key_manager_y_i);
            let key_registry_y_i =
                KeyRegistry::add_longterm_bundle(key_registry_y, my_id, key_bundle.clone());

            self.key_store
                .set_key_manager(&key_manager_y_i)
                .await
                .map_err(IdentityError::KeyStore)?;
            self.key_store
                .set_key_registry(&key_registry_y_i)
                .await
                .map_err(IdentityError::KeyStore)?;

            key_bundle
        } else {
            KeyManager::prekey_bundle(&key_manager_y)
        };

        Ok(Member::new(my_id, key_bundle))
    }

    /// Register a member with long-term key bundle material.
    pub async fn register_member(
        &mut self,
        member: &Member,
    ) -> Result<(), IdentityError<ID, K, M, C>> {
        member.key_bundle().verify()?;

        let y = self
            .key_store
            .key_registry()
            .await
            .map_err(IdentityError::KeyStore)?;

        // @TODO: Setting longterm bundle should overwrite previous one if this is newer.
        let y_ii = KeyRegistry::add_longterm_bundle(y, member.id(), member.key_bundle().clone());

        self.key_store
            .set_key_registry(&y_ii)
            .await
            .map_err(IdentityError::KeyStore)?;

        Ok(())
    }

    /// Check if my latest key bundle has expired.
    pub async fn key_bundle_expired(&self) -> bool {
        now() - self.my_keys_rotated_at > self.pre_key_rotate_after.as_secs()
    }

    /// Forge a key bundle message containing my latest key bundle.
    ///
    /// Note: key bundle will be rotated if the latest is reaching it's configured expiry date.
    pub async fn key_bundle(&mut self) -> Result<M, IdentityError<ID, K, M, C>> {
        let me = self.me().await?;
        let args = SpacesArgs::KeyBundle {
            key_bundle: me.key_bundle().clone(),
        };
        let message = self
            .key_store
            .forge(args)
            .await
            .map_err(IdentityError::Forge)?;
        Ok(message)
    }

    /// Process a key bundle received from the network.
    pub async fn process_key_bundle(
        &mut self,
        author: ActorId,
        key_bundle: &LongTermKeyBundle,
    ) -> Result<(), IdentityError<ID, K, M, C>> {
        key_bundle.verify()?;

        // @TODO: validate that the key bundle was indeed created by the correct author.

        let member = Member::new(author, key_bundle.clone());
        self.register_member(&member).await?;
        Ok(())
    }

    pub async fn forge(
        &mut self,
        args: SpacesArgs<ID, C>,
    ) -> Result<M, IdentityError<ID, K, M, C>> {
        self.key_store
            .forge(args)
            .await
            .map_err(IdentityError::Forge)
    }

    pub async fn key_manager(&self) -> Result<KeyManagerState, IdentityError<ID, K, M, C>> {
        self.key_store
            .key_manager()
            .await
            .map_err(IdentityError::KeyStore)
    }

    pub async fn key_registry(
        &self,
    ) -> Result<KeyRegistryState<ActorId>, IdentityError<ID, K, M, C>> {
        self.key_store
            .key_registry()
            .await
            .map_err(IdentityError::KeyStore)
    }
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum IdentityError<ID, K, M, C>
where
    ID: SpaceId,
    K: KeyStore + Forge<ID, M, C>,
    C: Conditions,
{
    #[error("{0}")]
    Forge(<K as Forge<ID, M, C>>::Error),

    #[error(transparent)]
    KeyManager(#[from] KeyManagerError),

    #[error(transparent)]
    Rng(#[from] RngError),

    #[error(transparent)]
    KeyBundle(#[from] KeyBundleError),

    #[error("received long-term key bundle for {0} on message signed by unexpected author {1}")]
    KeyBundleAuthor(ActorId, ActorId),

    #[error("{0}")]
    KeyStore(<K as KeyStore>::Error),

    #[error(
        "identity key unexpectedly rotated which will result in loss of access to existing spaces"
    )]
    IdentitySecretRotated,

    #[error(
        "private key unexpectedly rotated which will result in loss of access to existing spaces"
    )]
    PrivateKeyRotated,
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use p2panda_core::PrivateKey;
    use p2panda_encryption::Rng;
    use p2panda_encryption::crypto::x25519::SecretKey;
    use p2panda_encryption::key_bundle::LongTermKeyBundle;
    use p2panda_encryption::key_registry::KeyRegistry;
    use p2panda_encryption::traits::{KeyBundle, PreKeyRegistry};

    use crate::identity::IdentityError;
    use crate::message::SpacesArgs;
    use crate::test_utils::TestKeyStore;
    use crate::traits::message::{AuthoredMessage, SpacesMessage};
    use crate::{ActorId, Config, Credentials};

    use super::IdentityManager;

    #[tokio::test]
    async fn identity_secret_rotated() {
        let rng = Rng::from_seed([1; 32]);
        let credentials = Credentials::new(&rng).unwrap();
        let mut config = Config::new(&credentials);
        let key_store: TestKeyStore<i32> = TestKeyStore::new(&config, &rng).unwrap();

        // Rotate identity secret
        config.credentials.identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let identity_manager = IdentityManager::new(key_store, &config, &rng)
            .await
            .unwrap();
        assert_matches!(
            identity_manager.validate().await,
            Err(IdentityError::IdentitySecretRotated)
        );
    }

    #[tokio::test]
    async fn private_key_rotated() {
        let rng = Rng::from_seed([1; 32]);
        let credentials = Credentials::new(&rng).unwrap();
        let mut config = Config::new(&credentials);
        let key_store: TestKeyStore<i32> = TestKeyStore::new(&config, &rng).unwrap();

        // Rotate private key
        let private_key = PrivateKey::from_bytes(&rng.random_array().unwrap());
        config.credentials.private_key = private_key;
        let identity_manager = IdentityManager::new(key_store, &config, &rng)
            .await
            .unwrap();
        assert_matches!(
            identity_manager.validate().await,
            Err(IdentityError::PrivateKeyRotated)
        );
    }

    #[tokio::test]
    async fn me_returns_valid_member() {
        let rng = Rng::from_seed([1; 32]);
        let credentials = Credentials::new(&rng).unwrap();
        let config = Config::new(&credentials);
        let key_store: TestKeyStore<i32> = TestKeyStore::new(&config, &rng).unwrap();
        let mut identity_manager = IdentityManager::new(key_store, &config, &rng)
            .await
            .unwrap();

        let me = identity_manager.me().await.unwrap();
        let bundle: &LongTermKeyBundle = me.key_bundle();
        let actor_id: ActorId = credentials.public_key().into();

        assert_eq!(me.id(), actor_id);
        assert!(bundle.verify().is_ok());
    }

    #[tokio::test]
    async fn key_bundle_message_forged() {
        let rng = Rng::from_seed([1; 32]);
        let credentials = Credentials::new(&rng).unwrap();
        let config = Config::new(&credentials);
        let key_store: TestKeyStore<i32> = TestKeyStore::new(&config, &rng).unwrap();
        let mut identity_manager = IdentityManager::new(key_store, &config, &rng)
            .await
            .unwrap();

        let msg = identity_manager.key_bundle().await.unwrap();

        let actor_id: ActorId = credentials.private_key().public_key().into();
        assert_eq!(msg.author(), actor_id);
        match msg.args() {
            SpacesArgs::KeyBundle { key_bundle } => {
                assert!(key_bundle.verify().is_ok());
            }
            _ => panic!("expected key bundle message"),
        }
    }

    #[tokio::test]
    async fn process_key_bundle_registers_member() {
        let alice_rng = Rng::from_seed([1; 32]);
        let alice_credentials = Credentials::new(&alice_rng).unwrap();
        let alice_config = Config::new(&alice_credentials);
        let alice_key_store: TestKeyStore<i32> =
            TestKeyStore::new(&alice_config, &alice_rng).unwrap();
        let mut alice_identity_manager =
            IdentityManager::new(alice_key_store, &alice_config, &alice_rng)
                .await
                .unwrap();

        let bob_rng = Rng::from_seed([2; 32]);
        let bob_credentials = Credentials::new(&bob_rng).unwrap();
        let bob_config = Config::new(&bob_credentials);
        let bob_key_store: TestKeyStore<i32> = TestKeyStore::new(&bob_config, &bob_rng).unwrap();
        let mut bob_identity_manager = IdentityManager::new(bob_key_store, &bob_config, &bob_rng)
            .await
            .unwrap();
        let bob_id = bob_credentials.public_key().into();

        let bob_member = bob_identity_manager.me().await.unwrap();
        let bob_bundle = bob_member.key_bundle();
        alice_identity_manager
            .process_key_bundle(bob_id, bob_bundle)
            .await
            .unwrap();

        let key_registry_y = alice_identity_manager.key_registry().await.unwrap();
        let (_, bundle): (_, Option<LongTermKeyBundle>) =
            KeyRegistry::key_bundle(key_registry_y, &bob_id).unwrap();
        assert!(bundle.is_some());
        let bundle_identity_key = bundle.unwrap().identity_key().to_owned();
        assert_eq!(bundle_identity_key, *bob_bundle.identity_key());
        assert_eq!(
            bundle_identity_key,
            bob_credentials.identity_secret().public_key().unwrap()
        );
    }

    #[tokio::test]
    async fn me_rotates_key_bundle_when_expired() {
        let alice_rng = Rng::from_seed([1; 32]);
        let alice_credentials = Credentials::new(&alice_rng).unwrap();
        let alice_config = Config::new(&alice_credentials);
        let alice_key_store: TestKeyStore<i32> =
            TestKeyStore::new(&alice_config, &alice_rng).unwrap();
        let mut alice_identity_manager =
            IdentityManager::new(alice_key_store, &alice_config, &alice_rng)
                .await
                .unwrap();

        let alice_1 = alice_identity_manager.me().await.unwrap();
        let bundle_1 = alice_1.key_bundle().clone();

        // Force expiry
        {
            alice_identity_manager.my_keys_rotated_at = 0;
        }

        let alice_2 = alice_identity_manager.me().await.unwrap();
        let bundle_2 = alice_2.key_bundle().clone();

        assert!(bundle_1.verify().is_ok());
        assert!(bundle_2.verify().is_ok());
        assert_ne!(
            bundle_1.signed_prekey(),
            bundle_2.signed_prekey(),
            "rotation should produce a new pre-key"
        );
    }
}
