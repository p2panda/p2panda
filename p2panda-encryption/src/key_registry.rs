// SPDX-License-Identifier: MIT OR Apache-2.0

//! Manager for public key material of other members.
//!
//! Peers should actively look for fresh key bundles in the network, check for invalid or expired
//! ones and automatically choose the latest for groups.
use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt::Debug;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::x25519::PublicKey;
use crate::key_bundle::{KeyBundleError, LongTermKeyBundle, OneTimeKeyBundle, latest_key_bundle};
use crate::traits::{IdentityHandle, IdentityRegistry, KeyBundle, PreKeyRegistry};

/// Key registry to maintain public key material of other members we've collected.
#[derive(Clone, Debug)]
pub struct KeyRegistry<ID> {
    _marker: PhantomData<ID>,
}

/// Serializable state of key registry (for persistence).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyRegistryState<ID>
where
    ID: IdentityHandle,
{
    identities: HashMap<ID, PublicKey>,
    onetime_bundles: HashMap<ID, Vec<OneTimeKeyBundle>>,
    longterm_bundles: HashMap<ID, Vec<LongTermKeyBundle>>,
}

impl<ID> KeyRegistry<ID>
where
    ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
{
    /// Returns newly initialised key-registry state.
    pub fn init() -> KeyRegistryState<ID> {
        KeyRegistryState {
            identities: HashMap::new(),
            onetime_bundles: HashMap::new(),
            longterm_bundles: HashMap::new(),
        }
    }

    /// Adds long-term pre-key bundle to the registry.
    ///
    /// This throws an error if an expired or invalid bundle was added.
    pub fn add_longterm_bundle(
        mut y: KeyRegistryState<ID>,
        id: ID,
        key_bundle: LongTermKeyBundle,
    ) -> Result<KeyRegistryState<ID>, KeyRegistryError> {
        key_bundle.verify()?;
        let existing = y.identities.insert(id, *key_bundle.identity_key());
        if let Some(existing) = existing {
            // Sanity check.
            assert_eq!(&existing, key_bundle.identity_key());
        }
        y.longterm_bundles
            .entry(id)
            .and_modify(|bundles| bundles.push(key_bundle.clone()))
            .or_insert(vec![key_bundle]);
        Ok(y)
    }

    /// Adds one-time pre-key bundle to the registry.
    ///
    /// This throws an error if an expired or invalid bundle was added.
    pub fn add_onetime_bundle(
        mut y: KeyRegistryState<ID>,
        id: ID,
        key_bundle: OneTimeKeyBundle,
    ) -> Result<KeyRegistryState<ID>, KeyRegistryError> {
        key_bundle.verify()?;
        let existing = y.identities.insert(id, *key_bundle.identity_key());
        if let Some(existing) = existing {
            // Sanity check.
            assert_eq!(&existing, key_bundle.identity_key());
        }
        y.onetime_bundles
            .entry(id)
            .and_modify(|bundles| bundles.push(key_bundle.clone()))
            .or_insert(vec![key_bundle]);
        Ok(y)
    }
}

impl<ID> PreKeyRegistry<ID, OneTimeKeyBundle> for KeyRegistry<ID>
where
    ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
{
    type State = KeyRegistryState<ID>;

    type Error = Infallible;

    fn key_bundle(
        mut y: Self::State,
        id: &ID,
    ) -> Result<(Self::State, Option<OneTimeKeyBundle>), Self::Error> {
        let bundle = y
            .onetime_bundles
            .get_mut(id)
            .and_then(|bundles| bundles.pop());
        Ok((y, bundle))
    }
}

impl<ID> PreKeyRegistry<ID, LongTermKeyBundle> for KeyRegistry<ID>
where
    ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
{
    type State = KeyRegistryState<ID>;

    type Error = KeyRegistryError;

    fn key_bundle(
        y: Self::State,
        id: &ID,
    ) -> Result<(Self::State, Option<LongTermKeyBundle>), Self::Error> {
        let Some(bundles) = y.longterm_bundles.get(id) else {
            return Ok((y, None));
        };

        let valid_bundle = latest_key_bundle(bundles).cloned();

        // Even though key bundles are available we couldn't find any non-expired ones.
        if !bundles.is_empty() && valid_bundle.is_none() {
            return Err(KeyRegistryError::KeyBundlesExpired);
        }

        Ok((y, valid_bundle))
    }
}

impl<ID> IdentityRegistry<ID, KeyRegistryState<ID>> for KeyRegistry<ID>
where
    ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
{
    type Error = Infallible;

    fn identity_key(y: &KeyRegistryState<ID>, id: &ID) -> Result<Option<PublicKey>, Self::Error> {
        let key = y.identities.get(id).cloned();
        Ok(key)
    }
}

#[derive(Debug, Error)]
pub enum KeyRegistryError {
    #[error(transparent)]
    KeyBundle(#[from] KeyBundleError),

    #[error("all available key bundles of this member expired")]
    KeyBundlesExpired,
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::Rng;
    use crate::crypto::x25519::SecretKey;
    use crate::key_bundle::{Lifetime, LongTermKeyBundle, PreKey};
    use crate::key_manager::KeyManager;
    use crate::traits::{PreKeyManager, PreKeyRegistry};

    use super::KeyRegistry;

    #[test]
    fn latest_key_bundle() {
        let rng = Rng::from_seed([1; 32]);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX EPOCH!")
            .as_secs();

        let member_id = 0;

        let identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());

        // Initialize key manager with first bundle.
        let keys = KeyManager::init(
            &identity_secret,
            Lifetime::from_range(now - 60, now + 60),
            &rng,
        )
        .unwrap();
        let bundle_1 = KeyManager::prekey_bundle(&keys).unwrap();

        // Generate second bundle (which expires earlier).
        let keys = KeyManager::rotate_prekey(keys, Lifetime::from_range(now - 60, now + 30), &rng)
            .unwrap();
        let bundle_2 = KeyManager::prekey_bundle(&keys).unwrap();

        // Initialize key registry and register both bundles there.
        let pki = {
            let y = KeyRegistry::init();
            let y = KeyRegistry::add_longterm_bundle(y, member_id, bundle_1.clone()).unwrap();
            let y = KeyRegistry::add_longterm_bundle(y, member_id, bundle_2).unwrap();
            y
        };

        // Registry returns bundle which has the "furthest" expiry date.
        assert_eq!(
            KeyRegistry::key_bundle(pki, &member_id).unwrap().1,
            Some(bundle_1)
        );
    }

    #[test]
    fn invalid_bundles() {
        let rng = Rng::from_seed([1; 32]);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX EPOCH!")
            .as_secs();

        let member_id = 0;

        let identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());

        let prekey_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let prekey = PreKey::new(
            prekey_secret.public_key().unwrap(),
            Lifetime::from_range(now - 60, now - 30),
        );
        let prekey_signature = prekey.sign(&identity_secret, &rng).unwrap();

        let invalid_bundle = LongTermKeyBundle::new(
            identity_secret.public_key().unwrap(),
            prekey,
            prekey_signature,
        );

        let pki = KeyRegistry::init();
        assert!(KeyRegistry::add_longterm_bundle(pki, member_id, invalid_bundle.clone()).is_err());
    }
}
