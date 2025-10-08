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

/// Serializable state of key manager (for persistence).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyManagerState {
    identity_secret: SecretKey,
    identity_key: PublicKey,
    prekeys: HashMap<PreKeyId, PreKeyState>,
    onetime_secrets: HashMap<OneTimePreKeyId, (PreKeyId, SecretKey)>,
    onetime_next_id: OneTimePreKeyId,
}

impl KeyManagerState {
    fn latest_prekey(&self) -> Option<PreKeyState> {
        let prekeys = self.prekeys.values().map(|state| &state.prekey).collect();
        let latest = latest_prekey(prekeys);
        latest.map(|prekey| {
            self.prekeys
                .get(prekey.key())
                .expect("we know the item exists in the set")
                .clone()
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreKeyState {
    prekey: PreKey,
    signature: XSignature,
    secret: SecretKey,
}

impl PreKeyState {
    pub fn init(
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
    /// Returns newly initialised key-manager state, holding our identity secret and a new signed
    /// pre-key secret which can be used to generate key-bundles.
    pub fn init(
        identity_secret: &SecretKey,
        lifetime: Lifetime,
        rng: &Rng,
    ) -> Result<KeyManagerState, KeyManagerError> {
        let prekey = PreKeyState::init(identity_secret, lifetime, rng)?;

        Ok(KeyManagerState {
            identity_key: identity_secret.public_key()?,
            identity_secret: identity_secret.clone(),
            prekeys: HashMap::from([(prekey.id(), prekey)]),
            onetime_secrets: HashMap::new(),
            onetime_next_id: 0,
        })
    }

    /// Remove all expired key bundles from manager.
    pub fn remove_expired(mut y: KeyManagerState) -> KeyManagerState {
        // Remove all expired pre keys.
        y.prekeys = y
            .prekeys
            .into_iter()
            .filter(|(_, prekey)| prekey.prekey.verify_lifetime().is_ok())
            .collect();

        // Remove one-time bundles which do not have a valid pre-key anymore.
        y.onetime_secrets = y
            .onetime_secrets
            .into_iter()
            .filter(|(_, (prekey_id, _))| y.prekeys.contains_key(prekey_id))
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
        let prekey = PreKeyState::init(&y.identity_secret, lifetime, rng)?;
        y.prekeys.insert(prekey.id(), prekey);
        Ok(y)
    }

    /// Returns latest, public long-term key-bundle which can be published on the network.
    ///
    /// Note that key bundles can be expired and thus invalid, this method will return an error in
    /// this case and applications need to generate new ones when necessary.
    fn prekey_bundle(y: &Self::State) -> Result<LongTermKeyBundle, Self::Error> {
        y.latest_prekey()
            .map(|latest| LongTermKeyBundle::new(y.identity_key, latest.prekey, latest.signature))
            .ok_or(KeyManagerError::NoPreKeysAvailable)
    }

    /// Creates a new public one-time key-bundle.
    fn generate_onetime_bundle(
        mut y: Self::State,
        rng: &Rng,
    ) -> Result<(Self::State, OneTimeKeyBundle), Self::Error> {
        let latest = y
            .latest_prekey()
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
        let state = KeyManager::init(&identity_secret, Lifetime::default(), &rng).unwrap();

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

        let y = KeyManager::init(
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
        let y = KeyManager::init(
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
