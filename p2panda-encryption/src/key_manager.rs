// SPDX-License-Identifier: MIT OR Apache-2.0

//! Manager for our own secret key material.
//!
//! Peers should automatically rotate their key bundles if the lifetime is close to expiring. It is
//! recommended to do this in good time before the actual expiration date to allow others to
//! receive it even when the network is unstable or peers are longer offline.
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::x25519::{PublicKey, SecretKey, X25519Error};
use crate::crypto::xeddsa::{XEdDSAError, XSignature};
use crate::crypto::{Rng, RngError};
use crate::key_bundle::{
    Lifetime, LongTermKeyBundle, OneTimeKeyBundle, OneTimePreKey, OneTimePreKeyId, PreKey,
    PreKeyId, latest_prekey,
};
use crate::traits::{IdentityManager, PreKeyManager};

/// Key manager to maintain secret key material (like our identity key) and to generate signed
/// public pre-key bundles.
#[derive(Clone, Debug)]
pub struct KeyManager;

/// Serializable state of key manager.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyManagerState {
    identity_secret: SecretKey,
    identity_key: PublicKey,
    prekeys: PreKeyBundlesState,
    // @TODO(adz): Could make sense to factor out one-time secrets into a similar structure like
    // pre-keys as well, so they can be independently handled in a storage layer.
    onetime_secrets: HashMap<OneTimePreKeyId, (PreKeyId, SecretKey)>,
    onetime_next_id: OneTimePreKeyId,
}

impl KeyManagerState {
    pub fn prekey_bundles(&self) -> &PreKeyBundlesState {
        &self.prekeys
    }
}

/// Collection of all known, publishable pre-keys with their regarding secrets and signatures in
/// this key manager. Offers a form of "garbage collection" to automatically remove expired
/// pre-keys.
///
/// This can be serialized and independently stored from the identity secrets.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PreKeyBundlesState(HashMap<PreKeyId, PreKeyBundle>);

impl PreKeyBundlesState {
    /// Initializes and returns new instance of pre-key bundles state.
    fn new() -> Self {
        Self::default()
    }

    /// Returns latest pre-key if valid and available and `None` otherwise.
    fn latest(&self) -> Option<PreKeyBundle> {
        let prekeys = self.0.values().map(|state| &state.prekey).collect();
        let latest = latest_prekey(prekeys);
        latest.map(|prekey| {
            self.0
                .get(prekey.key())
                .expect("we know the item exists in the set")
                .clone()
        })
    }

    fn contains(&self, id: &PreKeyId) -> bool {
        self.0.contains_key(id)
    }

    #[allow(unused)]
    fn len(&self) -> usize {
        self.0.len()
    }

    fn get(&self, id: &PreKeyId) -> Option<&PreKeyBundle> {
        self.0.get(id)
    }

    /// Insert pre-key bundle into set.
    fn insert(mut self, bundle: PreKeyBundle) -> Self {
        self.0.insert(bundle.id(), bundle);
        self
    }

    /// Remove all expired key bundles from manager.
    #[allow(clippy::manual_retain)]
    fn remove_expired(self) -> Self {
        Self(
            self.0
                .into_iter()
                .filter(|(_, prekey)| prekey.prekey.verify_lifetime().is_ok())
                .collect(),
        )
    }
}

/// Extended pre-key struct holding the public and secret parts and signature, authenticating the
/// pre-key with an identity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreKeyBundle {
    prekey: PreKey,
    signature: XSignature,
    secret: SecretKey,
}

impl PreKeyBundle {
    /// Generates, signs new pre-key and returns struct holding signature and pre-key secret
    /// next to the public part.
    pub fn new(
        identity_secret: &SecretKey,
        lifetime: Lifetime,
        rng: &Rng,
    ) -> Result<Self, KeyManagerError> {
        let secret = SecretKey::from_bytes(rng.random_array()?);
        let prekey = PreKey::new(secret.public_key()?, lifetime);
        let signature = prekey.sign(identity_secret, rng)?;

        Ok(Self {
            prekey,
            signature,
            secret,
        })
    }

    pub fn id(&self) -> PreKeyId {
        *self.prekey.key()
    }

    pub fn lifetime(&self) -> &Lifetime {
        self.prekey.lifetime()
    }
}

impl KeyManager {
    /// Returns newly initialised key-manager state, holding our identity secret.
    pub fn init(identity_secret: &SecretKey) -> Result<KeyManagerState, KeyManagerError> {
        Ok(KeyManagerState {
            identity_key: identity_secret.public_key()?,
            identity_secret: identity_secret.clone(),
            prekeys: PreKeyBundlesState::new(),
            onetime_secrets: HashMap::new(),
            onetime_next_id: 0,
        })
    }

    /// Returns newly initialised key-manager state, holding our identity secret with existing
    /// pre-key bundles.
    pub fn init_from_prekey_bundles(
        identity_secret: &SecretKey,
        prekeys: PreKeyBundlesState,
    ) -> Result<KeyManagerState, KeyManagerError> {
        Ok(KeyManagerState {
            identity_key: identity_secret.public_key()?,
            identity_secret: identity_secret.clone(),
            prekeys,
            onetime_secrets: HashMap::new(),
            onetime_next_id: 0,
        })
    }

    /// Returns newly initialised key-manager state, holding our identity secret and an
    /// automatically generated, first pre-key secret which can be used to generate key-bundles.
    #[cfg(any(test, feature = "test_utils"))]
    pub fn init_and_generate_prekey(
        identity_secret: &SecretKey,
        lifetime: Lifetime,
        rng: &Rng,
    ) -> Result<KeyManagerState, KeyManagerError> {
        let bundle = PreKeyBundle::new(identity_secret, lifetime, rng)?;
        let prekeys = PreKeyBundlesState::new().insert(bundle);

        Ok(KeyManagerState {
            identity_key: identity_secret.public_key()?,
            identity_secret: identity_secret.clone(),
            prekeys,
            onetime_secrets: HashMap::new(),
            onetime_next_id: 0,
        })
    }

    /// Remove all expired pre-key bundles from manager.
    #[allow(clippy::manual_retain)]
    pub fn remove_expired(mut y: KeyManagerState) -> KeyManagerState {
        // Remove all expired pre keys.
        y.prekeys = y.prekeys.remove_expired();

        // Remove one-time bundles which do not have a valid pre-key anymore.
        y.onetime_secrets = y
            .onetime_secrets
            .into_iter()
            .filter(|(_, (prekey_id, _))| y.prekeys.contains(prekey_id))
            .collect();

        y
    }
}

impl IdentityManager<KeyManagerState> for KeyManager {
    /// Returns identity key secret.
    fn identity_secret(y: &KeyManagerState) -> &SecretKey {
        &y.identity_secret
    }
}

impl PreKeyManager for KeyManager {
    type State = KeyManagerState;

    type Error = KeyManagerError;

    /// Returns long-term pre-key secret by id.
    ///
    /// Throws an error if pre-key was not found (for example because it expired and was removed).
    fn prekey_secret<'a>(
        y: &'a Self::State,
        id: &'a PreKeyId,
    ) -> Result<&'a SecretKey, Self::Error> {
        match y.prekeys.get(id) {
            Some(prekey) => Ok(&prekey.secret),
            None => Err(KeyManagerError::UnknownPreKeySecret(*id)),
        }
    }

    /// Generates a new long-term pre-key secret with the given lifetime.
    fn rotate_prekey(
        mut y: Self::State,
        lifetime: Lifetime,
        rng: &Rng,
    ) -> Result<Self::State, Self::Error> {
        let prekey = PreKeyBundle::new(&y.identity_secret, lifetime, rng)?;
        y.prekeys = y.prekeys.insert(prekey);
        Ok(y)
    }

    /// Returns latest, public long-term key-bundle which can be published on the network.
    ///
    /// Note that key bundles can be expired and thus invalid, this method will return an error in
    /// this case and applications need to generate new ones when necessary.
    fn prekey_bundle(y: &Self::State) -> Result<LongTermKeyBundle, Self::Error> {
        y.prekeys
            .latest()
            .map(|latest| LongTermKeyBundle::new(y.identity_key, latest.prekey, latest.signature))
            .ok_or(KeyManagerError::NoPreKeysAvailable)
    }

    /// Creates a new public one-time key-bundle.
    fn generate_onetime_bundle(
        mut y: Self::State,
        rng: &Rng,
    ) -> Result<(Self::State, OneTimeKeyBundle), Self::Error> {
        let latest = y
            .prekeys
            .latest()
            .ok_or(KeyManagerError::NoPreKeysAvailable)?;

        let onetime_secret = SecretKey::from_bytes(rng.random_array()?);
        let onetime_key = OneTimePreKey::new(onetime_secret.public_key()?, y.onetime_next_id);

        {
            let existing_key = y
                .onetime_secrets
                .insert(onetime_key.id(), (latest.id(), onetime_secret));
            // Sanity check.
            assert!(
                existing_key.is_none(),
                "should never insert same id more than once"
            );
        };

        let bundle = OneTimeKeyBundle::new(
            y.identity_key,
            latest.prekey,
            latest.signature,
            Some(onetime_key),
        );

        y.onetime_next_id += 1;

        Ok((y, bundle))
    }

    /// Returns one-time pre-key secret used by a sender during X3DH.
    ///
    /// Throws an error when requested pre-key secret is unknown (and thus probably was already
    /// used once).
    ///
    /// Returns none when this key-manager doesn't have any one-time pre-keys. New ones can be
    /// created by calling `generate_onetime_bundle`.
    fn use_onetime_secret(
        mut y: Self::State,
        id: OneTimePreKeyId,
    ) -> Result<(Self::State, Option<SecretKey>), Self::Error> {
        match y.onetime_secrets.remove(&id) {
            Some(secret) => Ok((y, Some(secret.1))),
            None => Err(KeyManagerError::UnknownOneTimeSecret(id)),
        }
    }
}

#[derive(Debug, Error)]
pub enum KeyManagerError {
    #[error(transparent)]
    Rng(#[from] RngError),

    #[error(transparent)]
    XEdDSA(#[from] XEdDSAError),

    #[error(transparent)]
    X25519(#[from] X25519Error),

    #[error("could not find one-time pre-key secret with id {0}")]
    UnknownOneTimeSecret(OneTimePreKeyId),

    #[error("could not find pre-key secret with id {0}")]
    UnknownPreKeySecret(PreKeyId),

    #[error("no valid pre-keys available, they are either expired or too early")]
    NoPreKeysAvailable,
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::crypto::Rng;
    use crate::crypto::x25519::SecretKey;
    use crate::key_bundle::Lifetime;
    use crate::key_manager::KeyManagerError;
    use crate::traits::KeyBundle;

    use super::{KeyManager, PreKeyManager};

    #[test]
    fn generate_onetime_keys() {
        let rng = Rng::from_seed([1; 32]);

        let identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let state =
            KeyManager::init_and_generate_prekey(&identity_secret, Lifetime::default(), &rng)
                .unwrap();

        let (state, bundle_1) = KeyManager::generate_onetime_bundle(state, &rng).unwrap();
        let (state, bundle_2) = KeyManager::generate_onetime_bundle(state, &rng).unwrap();

        // Prekey stays the same between each bundle and match the secret key.
        assert_eq!(
            bundle_1.signed_prekey(),
            &KeyManager::prekey_secret(&state, bundle_1.signed_prekey())
                .expect("non-expired prekey exists")
                .public_key()
                .unwrap()
        );
        assert_eq!(bundle_1.signed_prekey(), bundle_2.signed_prekey());

        // Identity key matches the identity secret.
        assert_eq!(
            bundle_1.identity_key(),
            &identity_secret.public_key().unwrap()
        );
        assert_eq!(
            bundle_2.identity_key(),
            &identity_secret.public_key().unwrap()
        );

        // Signature is correct.
        assert!(bundle_1.verify().is_ok());
        assert!(bundle_2.verify().is_ok());

        let (state, onetime_secret_1) =
            KeyManager::use_onetime_secret(state, bundle_1.onetime_prekey_id().unwrap()).unwrap();
        let (state, onetime_secret_2) =
            KeyManager::use_onetime_secret(state, bundle_2.onetime_prekey_id().unwrap()).unwrap();

        // Secrets got removed from state.
        assert_eq!(state.onetime_secrets.len(), 0);

        // Retrieving unknown one-time prekeys throws an error.
        assert!(KeyManager::use_onetime_secret(state.clone(), 42).is_err());

        // Re-retrieving known one-time prekeys throws an error.
        assert!(
            KeyManager::use_onetime_secret(state.clone(), bundle_1.onetime_prekey_id().unwrap())
                .is_err()
        );
        assert!(
            KeyManager::use_onetime_secret(state.clone(), bundle_2.onetime_prekey_id().unwrap())
                .is_err()
        );

        // One-time prekeys match the secret.
        assert_eq!(
            bundle_1.onetime_prekey().unwrap(),
            &onetime_secret_1.unwrap().public_key().unwrap()
        );
        assert_eq!(
            bundle_2.onetime_prekey().unwrap(),
            &onetime_secret_2.unwrap().public_key().unwrap()
        );

        // One-time prekeys are unique.
        assert_ne!(bundle_1.onetime_prekey(), bundle_2.onetime_prekey());
        assert_ne!(bundle_1.onetime_prekey_id(), bundle_2.onetime_prekey_id());
    }

    #[test]
    fn expired_prekey_bundles() {
        let rng = Rng::from_seed([1; 32]);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX EPOCH!")
            .as_secs();

        let identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());

        let y = KeyManager::init_and_generate_prekey(
            &identity_secret,
            Lifetime::from_range(now - 120, now - 60), // expired lifetime
            &rng,
        )
        .unwrap();

        // Current pre-key bundle is invalid.
        assert!(matches!(
            KeyManager::prekey_bundle(&y),
            Err(KeyManagerError::NoPreKeysAvailable)
        ));

        // Can't generate one-time key bundle with expired pre keys.
        assert!(matches!(
            KeyManager::generate_onetime_bundle(y.clone(), &rng),
            Err(KeyManagerError::NoPreKeysAvailable)
        ));

        // Generate a new one.
        let y_i = KeyManager::rotate_prekey(y, Lifetime::default(), &rng).unwrap();
        assert!(KeyManager::prekey_bundle(&y_i).is_ok());
    }

    #[test]
    fn garbage_collection() {
        let rng = Rng::from_seed([1; 32]);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX EPOCH!")
            .as_secs();

        let identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());

        // Initialise key manager with one invalid key bundle.
        let y = KeyManager::init_and_generate_prekey(
            &identity_secret,
            Lifetime::from_range(now - 120, now - 60), // expired lifetime
            &rng,
        )
        .unwrap();
        assert_eq!(y.prekeys.len(), 1);

        // Add second _valid_ key bundle.
        let y = KeyManager::rotate_prekey(y, Lifetime::default(), &rng).unwrap();
        assert_eq!(y.prekeys.len(), 2);

        // Remove all expired bundles.
        let y = KeyManager::remove_expired(y);
        assert_eq!(y.prekeys.len(), 1);
    }
}
