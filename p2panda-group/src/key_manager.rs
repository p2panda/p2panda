// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::x25519::{PublicKey, SecretKey, X25519Error};
use crate::crypto::xeddsa::{XEdDSAError, XSignature};
use crate::crypto::{Rng, RngError};
use crate::key_bundle::{
    Lifetime, LongTermKeyBundle, OneTimeKeyBundle, OneTimePreKey, OneTimePreKeyId, PreKey,
};
use crate::traits::{IdentityManager, PreKeyManager};

/// Key manager to maintain secret key material (like our identity key) and to generate signed
/// public pre-key bundles.
#[derive(Debug)]
pub struct KeyManager;

/// Serializable state of key manager (for persistance).
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(Clone))]
pub struct KeyManagerState {
    pub identity_secret: SecretKey,
    pub identity_key: PublicKey,
    pub prekey_secret: SecretKey,
    pub prekey: PreKey,
    pub prekey_signature: XSignature,
    pub onetime_secrets: HashMap<OneTimePreKeyId, SecretKey>,
    pub onetime_next_id: OneTimePreKeyId,
}

impl KeyManager {
    /// Returns newly initialised key-manager state, holding our identity secret and a new signed
    /// pre-key secret which can be used to generate key-bundles.
    pub fn init(
        identity_secret: &SecretKey,
        lifetime: Lifetime,
        rng: &Rng,
    ) -> Result<KeyManagerState, KeyManagerError> {
        let prekey_secret = SecretKey::from_bytes(rng.random_array()?);
        let prekey = PreKey::new(prekey_secret.public_key()?, lifetime);
        let prekey_signature = prekey.sign(identity_secret, rng)?;
        Ok(KeyManagerState {
            identity_key: identity_secret.public_key()?,
            identity_secret: identity_secret.clone(),
            prekey_secret,
            prekey_signature,
            prekey,
            onetime_secrets: HashMap::new(),
            onetime_next_id: 0,
        })
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

    /// Returns long-term pre-key secret.
    fn prekey_secret(y: &Self::State) -> &SecretKey {
        &y.prekey_secret
    }

    /// Generates a new long-term pre-key secret with the given lifetime.
    fn rotate_prekey(
        y: Self::State,
        lifetime: Lifetime,
        rng: &Rng,
    ) -> Result<Self::State, Self::Error> {
        Self::init(&y.identity_secret, lifetime, rng)
    }

    /// Returns public long-term key-bundle which can be published on the network.
    fn prekey_bundle(y: &Self::State) -> LongTermKeyBundle {
        LongTermKeyBundle::new(y.identity_key, y.prekey, y.prekey_signature)
    }

    /// Creates a new public one-time key-bundle.
    fn generate_onetime_bundle(
        mut y: Self::State,
        rng: &Rng,
    ) -> Result<(Self::State, OneTimeKeyBundle), Self::Error> {
        let onetime_secret = SecretKey::from_bytes(rng.random_array()?);
        let onetime_key = OneTimePreKey::new(onetime_secret.public_key()?, y.onetime_next_id);

        {
            let existing_key = y.onetime_secrets.insert(onetime_key.id(), onetime_secret);
            assert!(
                existing_key.is_none(),
                "should never insert same id more than once"
            );
        };

        let bundle = OneTimeKeyBundle::new(
            y.identity_key,
            y.prekey,
            y.prekey_signature,
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
            Some(secret) => Ok((y, Some(secret))),
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
}

#[cfg(test)]
mod tests {
    use crate::crypto::x25519::SecretKey;
    use crate::traits::KeyBundle;
    use crate::{crypto::Rng, key_bundle::Lifetime};

    use super::{KeyManager, PreKeyManager};

    #[test]
    fn generate_onetime_keys() {
        let rng = Rng::default();

        let identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let state = KeyManager::init(&identity_secret, Lifetime::default(), &rng).unwrap();

        let (state, bundle_1) = KeyManager::generate_onetime_bundle(state, &rng).unwrap();
        let (state, bundle_2) = KeyManager::generate_onetime_bundle(state, &rng).unwrap();

        // Prekey stays the same between each bundle and match the secret key.
        assert_eq!(
            bundle_1.signed_prekey(),
            &KeyManager::prekey_secret(&state).public_key().unwrap()
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

        // Retreiving unknown one-time prekeys throws an error.
        assert!(KeyManager::use_onetime_secret(state.clone(), 42).is_err());

        // Re-retreiving known one-time prekeys throws an error.
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
}
