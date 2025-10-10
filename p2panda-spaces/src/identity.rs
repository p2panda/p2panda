// SPDX-License-Identifier: MIT OR Apache-2.0

//! API for managing members and their key bundles.
use std::fmt::Debug;
use std::marker::PhantomData;

use p2panda_auth::traits::Conditions;
use p2panda_encryption::key_bundle::{KeyBundleError, Lifetime, LongTermKeyBundle};
use p2panda_encryption::key_manager::{KeyManager, KeyManagerError, KeyManagerState};
use p2panda_encryption::key_registry::{KeyRegistry, KeyRegistryError, KeyRegistryState};
use p2panda_encryption::traits::{KeyBundle, PreKeyManager};
use p2panda_encryption::{Rng, RngError};
use thiserror::Error;

use crate::event::Event;
use crate::member::Member;
use crate::message::SpacesArgs;
use crate::traits::SpaceId;
use crate::traits::forge::Forge;
use crate::traits::message::{AuthoredMessage, SpacesMessage};
use crate::traits::store::{KeyRegistryStore, KeySecretStore};
use crate::types::ActorId;
use crate::{Config, Credentials};

/// Manager for functionality relating to a peers identity, holds all cryptographic secrets for key
/// agreement and signatures.
///
/// Exposes an API for publishing and storing/retrieving key bundles, including rotating our own
/// when they expire, as well as methods for "forging" (constructing and signing messages) which
/// are signed with the peers private key.
///
/// **Warning:** Neither of a peers keys should be rotated individually, this would result in
/// undefined behavior. Rotating both keys is possible but will result in the loss of access to
/// existing spaces.
#[derive(Debug)]
pub struct IdentityManager<ID, K, M, C> {
    key_store: K,
    credentials: Credentials,
    config: Config,
    rng: Rng,
    _marker: PhantomData<(ID, M, C)>,
}

impl<ID, K, M, C> IdentityManager<ID, K, M, C>
where
    ID: SpaceId,
    K: KeySecretStore + KeyRegistryStore + Forge<ID, M, C> + Debug,
    M: AuthoredMessage + SpacesMessage<ID, C>,
    C: Conditions,
{
    pub async fn new(
        key_store: K,
        credentials: Credentials,
        config: &Config,
        rng: &Rng,
    ) -> Result<Self, IdentityError<ID, K, M, C>> {
        let rng = Rng::from_rng(rng)?;
        let manager = Self {
            credentials,
            key_store,
            config: config.clone(),
            rng,
            _marker: PhantomData,
        };
        Ok(manager)
    }

    /// The public key of the local actor.
    pub(crate) fn id(&self) -> ActorId {
        self.credentials.public_key().into()
    }

    /// The local actor id and their long-term key bundle.
    ///
    /// Note: Key bundle will be rotated if the latest is reaching it's configured expiry date.
    pub(crate) async fn me(&mut self) -> Result<Member, IdentityError<ID, K, M, C>> {
        Ok(Member::new(self.id(), self.key_bundle().await?))
    }

    /// Returns "latest", publishable key bundle of us or automatically generates a new one if
    /// either nothing was generated yet, if previous bundles expired or are about to be expired
    /// (given an additional "pessimistic" rotation window).
    async fn key_bundle(&mut self) -> Result<LongTermKeyBundle, IdentityError<ID, K, M, C>> {
        let key_manager_y = self.key_manager().await?;

        let valid_bundle = match KeyManager::prekey_bundle(&key_manager_y) {
            Ok(bundle) => bundle
                .lifetime()
                .verify_with_window(self.config.pre_key_rotate_after)
                .map_or(None, |_| Some(bundle)),
            Err(KeyManagerError::NoPreKeysAvailable) => None,
            Err(err) => return Err(err.into()),
        };

        if let Some(bundle) = valid_bundle {
            return Ok(bundle);
        }

        // Automatically rotate pre key.
        let key_manager_y_i = KeyManager::rotate_prekey(
            key_manager_y,
            Lifetime::new(self.config.pre_key_lifetime.as_secs()),
            &self.rng,
        )?;

        let key_registry_y = self.key_registry().await?;

        // Register our own key bundle.
        let key_bundle = KeyManager::prekey_bundle(&key_manager_y_i)?;
        let key_registry_y_i =
            KeyRegistry::add_longterm_bundle(key_registry_y, self.id(), key_bundle.clone())?;

        // Clean up expired key bundles ("garbage collection").
        let key_manager_y_ii = KeyManager::remove_expired(key_manager_y_i);
        let key_registry_y_ii = KeyRegistry::remove_expired(key_registry_y_i);

        // Persist new state in store.
        self.key_store
            .set_prekey_secrets(key_manager_y_ii.prekey_bundles())
            .await
            .map_err(IdentityError::KeyManagerStore)?;
        self.key_store
            .set_key_registry(&key_registry_y_ii)
            .await
            .map_err(IdentityError::KeyRegistryStore)?;

        Ok(key_bundle)
    }

    /// Returns `true` if my latest key bundle has expired or is about to expire.
    pub async fn key_bundle_expired(&self) -> Result<bool, IdentityError<ID, K, M, C>> {
        let key_manager_y = self.key_manager().await?;
        match KeyManager::prekey_bundle(&key_manager_y) {
            Ok(bundle) => Ok(bundle
                .lifetime()
                .verify_with_window(self.config.pre_key_rotate_after)
                .is_err()),
            Err(KeyManagerError::NoPreKeysAvailable) => Ok(true),
            Err(err) => Err(err.into()),
        }
    }

    /// Forge a key bundle message containing my latest key bundle.
    ///
    /// Note: Key bundle will be rotated if the latest is reaching it's configured expiry date.
    pub async fn key_bundle_message(&mut self) -> Result<M, IdentityError<ID, K, M, C>> {
        let args = SpacesArgs::KeyBundle {
            key_bundle: self.key_bundle().await?,
        };
        let message = self
            .key_store
            .forge(args)
            .await
            .map_err(IdentityError::Forge)?;
        Ok(message)
    }

    /// Register a member with long-term key bundle material.
    ///
    /// Throws an error if provided key bundle has an invalid signature or expired.
    //
    // @NOTE(adz): **Security:** This method does _only_ validate if the pre-key signature maps to
    // the given identity key but **not** if the member's handle / id is authentic. Applications
    // need to provide an authentication scheme and validate `Member` before calling this method to
    // prevent impersonation attacks.
    pub async fn register_member(
        &mut self,
        member: &Member,
    ) -> Result<(), IdentityError<ID, K, M, C>> {
        let pki = {
            let y = self.key_registry().await?;
            KeyRegistry::add_longterm_bundle(y, member.id(), member.key_bundle().clone())?
        };

        self.key_store
            .set_key_registry(&pki)
            .await
            .map_err(IdentityError::KeyRegistryStore)?;

        Ok(())
    }

    /// Process a key bundle received from the network.
    pub async fn process_key_bundle(
        &mut self,
        author: ActorId,
        key_bundle: &LongTermKeyBundle,
    ) -> Result<Event<ID, C>, IdentityError<ID, K, M, C>> {
        key_bundle.verify()?;
        let member = Member::new(author, key_bundle.clone());
        self.register_member(&member).await?;
        Ok(Event::KeyBundle { author })
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

    /// Assemble and return key manager state from persisted pre-key bundles and identity secret.
    pub async fn key_manager(&self) -> Result<KeyManagerState, IdentityError<ID, K, M, C>> {
        let prekeys = self
            .key_store
            .prekey_secrets()
            .await
            .map_err(IdentityError::KeyManagerStore)?;

        Ok(KeyManager::init_from_prekey_bundles(
            &self.credentials.identity_secret(),
            prekeys,
        )?)
    }

    pub async fn key_registry(
        &self,
    ) -> Result<KeyRegistryState<ActorId>, IdentityError<ID, K, M, C>> {
        self.key_store
            .key_registry()
            .await
            .map_err(IdentityError::KeyRegistryStore)
    }
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum IdentityError<ID, K, M, C>
where
    ID: SpaceId,
    K: KeySecretStore + KeyRegistryStore + Forge<ID, M, C>,
    C: Conditions,
{
    #[error("{0}")]
    Forge(<K as Forge<ID, M, C>>::Error),

    #[error(transparent)]
    KeyManager(#[from] KeyManagerError),

    #[error(transparent)]
    KeyRegistry(#[from] KeyRegistryError),

    #[error(transparent)]
    Rng(#[from] RngError),

    #[error(transparent)]
    KeyBundle(#[from] KeyBundleError),

    #[error("received long-term key bundle for {0} on message signed by unexpected author {1}")]
    KeyBundleAuthor(ActorId, ActorId),

    #[error("{0}")]
    KeyRegistryStore(<K as KeyRegistryStore>::Error),

    #[error("{0}")]
    KeyManagerStore(<K as KeySecretStore>::Error),
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use p2panda_encryption::Rng;
    use p2panda_encryption::key_bundle::LongTermKeyBundle;
    use p2panda_encryption::key_registry::KeyRegistry;
    use p2panda_encryption::traits::{KeyBundle, PreKeyRegistry};

    use crate::message::SpacesArgs;
    use crate::test_utils::{TestKeyStore, TestSpacesStore};
    use crate::traits::message::{AuthoredMessage, SpacesMessage};
    use crate::{ActorId, Config, Credentials};

    use super::IdentityManager;

    #[tokio::test]
    async fn me_returns_valid_member() {
        let rng = Rng::from_seed([1; 32]);
        let credentials = Credentials::from_rng(&rng).unwrap();
        let config = Config::default();
        let spaces_store = TestSpacesStore::new();
        let key_store: TestKeyStore<i32> = TestKeyStore::new(spaces_store, &credentials).unwrap();
        let mut identity_manager =
            IdentityManager::new(key_store, credentials.clone(), &config, &rng)
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
        let credentials = Credentials::from_rng(&rng).unwrap();
        let config = Config::default();
        let spaces_store = TestSpacesStore::new();
        let key_store: TestKeyStore<i32> = TestKeyStore::new(spaces_store, &credentials).unwrap();
        let mut identity_manager =
            IdentityManager::new(key_store, credentials.clone(), &config, &rng)
                .await
                .unwrap();

        let msg = identity_manager.key_bundle_message().await.unwrap();

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
        let alice_credentials = Credentials::from_rng(&alice_rng).unwrap();
        let alice_config = Config::default();
        let alice_key_store: TestKeyStore<i32> =
            TestKeyStore::new(TestSpacesStore::new(), &alice_credentials).unwrap();
        let mut alice_identity_manager = IdentityManager::new(
            alice_key_store,
            alice_credentials,
            &alice_config,
            &alice_rng,
        )
        .await
        .unwrap();

        let bob_rng = Rng::from_seed([2; 32]);
        let bob_credentials = Credentials::from_rng(&bob_rng).unwrap();
        let bob_config = Config::default();
        let bob_key_store: TestKeyStore<i32> =
            TestKeyStore::new(TestSpacesStore::new(), &bob_credentials).unwrap();
        let mut bob_identity_manager = IdentityManager::new(
            bob_key_store,
            bob_credentials.clone(),
            &bob_config,
            &bob_rng,
        )
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
        let alice_credentials = Credentials::from_rng(&alice_rng).unwrap();
        let alice_config = Config::default();
        let alice_key_store: TestKeyStore<i32> =
            TestKeyStore::new(TestSpacesStore::new(), &alice_credentials).unwrap();
        let mut alice_identity_manager = IdentityManager::new(
            alice_key_store,
            alice_credentials,
            &alice_config,
            &alice_rng,
        )
        .await
        .unwrap();

        let alice_1 = alice_identity_manager.me().await.unwrap();
        let bundle_1 = alice_1.key_bundle().clone();

        // Override max. lifetime of 90 days (default) with pre-rotation window to force rotation.
        alice_identity_manager.config.pre_key_rotate_after =
            Duration::from_secs(60 * 60 * 24 * 1024);

        // Make lifetime of next key longer to "win" over the previous one, in case it is still
        // considered valid due to a race condition (both keys can be generated "at the same time").
        alice_identity_manager.config.pre_key_lifetime = Duration::from_secs(60 * 60 * 24 * 2048);

        let alice_2 = alice_identity_manager.me().await.unwrap();
        let bundle_2 = alice_2.key_bundle().clone();

        // Key bundles are valid, we only forced the generate a new one to pessimistically already
        // distribute it, but the "old" one is still fine!
        assert!(bundle_1.verify().is_ok());
        assert!(bundle_2.verify().is_ok());

        assert_ne!(
            bundle_1.signed_prekey(),
            bundle_2.signed_prekey(),
            "rotation should produce a new pre-key"
        );
    }
}
