// SPDX-License-Identifier: MIT OR Apache-2.0

//! API for managing members and their key bundles.
use std::fmt::Debug;
use std::marker::PhantomData;

use p2panda_auth::traits::Conditions;
use p2panda_core::VerifyingKey;
use p2panda_encryption::key_bundle::{KeyBundleError, Lifetime, LongTermKeyBundle};
use p2panda_encryption::key_manager::{KeyManager, KeyManagerError, KeyManagerState};
use p2panda_encryption::key_registry::{KeyRegistry, KeyRegistryError, KeyRegistryState};
use p2panda_encryption::traits::{KeyBundle, PreKeyManager};
use p2panda_encryption::{Rng, RngError};
use thiserror::Error;

use crate::event::Event;
use crate::forge::Forge;
use crate::member::{AuthenticatedMember, MemberError};
use crate::{Config, Credentials, MemberId};

/// Manager for functionality relating to a peer's identity, holds all cryptographic secrets for key
/// agreement and signatures.
///
/// Exposes an API for publishing and storing/retrieving key bundles, including rotating our own
/// when they expire, as well as methods for "forging" (constructing and signing messages) which are
/// signed with the peer's private key.
///
/// **Warning:** Neither of a peer's keys should be rotated individually, this would result in
/// undefined behavior. Rotating both keys is possible but will result in the loss of access to
/// existing spaces.
#[derive(Debug)]
pub struct IdentityManager<F, C> {
    config: Config,
    _marker: PhantomData<(F, C)>,
}

impl<F, C> IdentityManager<F, C> {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            _marker: PhantomData,
        }
    }
}

impl<F, C> IdentityManager<F, C>
where
    F: Forge<C>,
    C: Conditions,
{
    /// Rotate our pre-key, register the new key bundle and return it together with our
    /// authenticated member type.
    ///
    /// This _always_ generates a new pre-key, callers need to check `key_bundle_status` first if
    /// rotation is actually due. Expired key bundles are removed (garbage collection).
    pub(crate) fn rotate_key_bundle(
        &self,
        key_manager_y: KeyManagerState,
        key_registry_y: KeyRegistryState<MemberId>,
        credentials: &Credentials,
        rng: &Rng,
    ) -> Result<
        (
            KeyManagerState,
            KeyRegistryState<MemberId>,
            AuthenticatedMember,
        ),
        IdentityError<F, C>,
    > {
        let key_manager_y_i = KeyManager::rotate_prekey(
            key_manager_y,
            Lifetime::new(self.config.pre_key_lifetime.as_secs()),
            rng,
        )?;

        // Register our own key bundle.
        let key_bundle = KeyManager::prekey_bundle(&key_manager_y_i)?;
        let key_registry_y_i = KeyRegistry::add_longterm_bundle(
            key_registry_y,
            credentials.verifying_key(),
            key_bundle.clone(),
        )?;

        // Clean up expired key bundles (garbage collection).
        let key_manager_y_ii = KeyManager::remove_expired(key_manager_y_i);
        let key_registry_y_ii = KeyRegistry::remove_expired(key_registry_y_i);

        let member = AuthenticatedMember::sign(rng, credentials, key_bundle)?;

        Ok((key_manager_y_ii, key_registry_y_ii, member))
    }

    /// Returns status of our latest key bundle.
    pub fn key_bundle_status(
        &self,
        key_manager_y: &KeyManagerState,
    ) -> Result<MemberStatus<LongTermKeyBundle>, IdentityError<F, C>> {
        match KeyManager::prekey_bundle(key_manager_y) {
            Ok(bundle) => {
                // Check expiry first, an expired bundle is always also within the rotation window.
                let has_expired = bundle.lifetime().verify().is_err();
                let about_to_expire = bundle
                    .lifetime()
                    .verify_with_window(self.config.pre_key_rotate_after)
                    .is_err();

                if has_expired {
                    Ok(MemberStatus::Expired)
                } else if about_to_expire {
                    Ok(MemberStatus::RotationDue(bundle))
                } else {
                    Ok(MemberStatus::Healthy(bundle))
                }
            }
            Err(KeyManagerError::NoPreKeysAvailable) => Ok(MemberStatus::NotAvailable),
            Err(err) => Err(err.into()),
        }
    }

    /// Register a member with long-term key bundle material.
    ///
    /// Throws an error if provided key bundle has an invalid signature or expired.
    pub fn process(
        &self,
        key_registry_y: KeyRegistryState<MemberId>,
        member: &AuthenticatedMember,
    ) -> Result<(KeyRegistryState<MemberId>, Event<C>), IdentityError<F, C>> {
        // 1. Claimed member id belongs to the associated key bundle and it's X3DH identity key.
        // 2. Key bundle's pre-key belongs to identity key.
        // 3. Key bundle has not expired.
        member.verify()?;

        let key_registry_y_i = KeyRegistry::add_longterm_bundle(
            key_registry_y,
            member.id(),
            member.key_bundle().clone(),
        )?;

        Ok((key_registry_y_i, Event::Member(member.clone())))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MemberStatus<T> {
    /// Requested key bundle is not available. Use `register_member` to manually add it or wait
    /// until it was synced.
    NotAvailable,

    /// Requested key bundle has expired and is not valid anymore.
    ///
    /// If this is our own key bundle, use `rotate_key_bundle` to generate a new key bundle and
    /// message to deliver it.
    ///
    /// If this is someone elses key bundle, use `register_member` to update it or wait until it was
    /// synced.
    Expired,

    /// Requested key bundle is valid but about to expire.
    ///
    /// If this is our own key bundle, use `rotate_key_bundle` to generate a new key bundle and
    /// message to deliver it.
    RotationDue(T),

    /// Valid key bundle.
    Healthy(T),
}

impl<T> MemberStatus<T> {
    pub fn is_valid(&self) -> bool {
        match self {
            Self::NotAvailable | Self::Expired => false,
            Self::Healthy(_) | Self::RotationDue(_) => true,
        }
    }

    pub fn is_rotation_due(&self) -> bool {
        match self {
            Self::NotAvailable | Self::Expired | Self::RotationDue(_) => true,
            Self::Healthy(_) => false,
        }
    }

    /// Returns the inner value if the key bundle is still valid (healthy or rotation due).
    pub fn into_valid(self) -> Option<T> {
        match self {
            Self::NotAvailable | Self::Expired => None,
            Self::Healthy(value) | Self::RotationDue(value) => Some(value),
        }
    }
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum IdentityError<F, C>
where
    F: Forge<C>,
    C: Conditions,
{
    #[error("{0}")]
    Forge(F::Error),

    #[error(transparent)]
    KeyManager(#[from] KeyManagerError),

    #[error(transparent)]
    KeyRegistry(#[from] KeyRegistryError),

    #[error(transparent)]
    Rng(#[from] RngError),

    #[error(transparent)]
    KeyBundle(#[from] KeyBundleError),

    #[error(transparent)]
    Member(#[from] MemberError),

    #[error("received long-term key bundle for {0} on message signed by unexpected author {1}")]
    KeyBundleAuthor(VerifyingKey, VerifyingKey),
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use p2panda_encryption::Rng;
    use p2panda_encryption::key_bundle::LongTermKeyBundle;
    use p2panda_encryption::key_manager::{KeyManager, KeyManagerState};
    use p2panda_encryption::key_registry::{KeyRegistry, KeyRegistryState};
    use p2panda_encryption::traits::{KeyBundle, PreKeyRegistry};

    use crate::test_utils::{TestConditions, TestForge};
    use crate::{Config, Credentials, MemberId};

    use super::{IdentityManager, MemberStatus};

    type TestIdentityManager = IdentityManager<TestForge, TestConditions>;

    fn init_keys(credentials: &Credentials) -> (KeyManagerState, KeyRegistryState<MemberId>) {
        let key_manager_y = KeyManager::init(&credentials.identity_secret()).unwrap();
        let key_registry_y = KeyRegistry::init();
        (key_manager_y, key_registry_y)
    }

    #[test]
    fn rotate_returns_valid_member() {
        let rng = Rng::from_seed([1; 32]);
        let credentials = Credentials::from_rng(&rng).unwrap();
        let identity_manager = TestIdentityManager::new(Config::default());

        let (key_manager_y, key_registry_y) = init_keys(&credentials);

        // No key bundle was generated yet.
        assert_eq!(
            identity_manager.key_bundle_status(&key_manager_y).unwrap(),
            MemberStatus::NotAvailable
        );

        let (key_manager_y, _, me) = identity_manager
            .rotate_key_bundle(key_manager_y, key_registry_y, &credentials, &rng)
            .unwrap();
        let bundle: &LongTermKeyBundle = me.key_bundle();
        let actor_id = credentials.verifying_key();

        assert_eq!(me.id(), actor_id);
        assert!(bundle.verify().is_ok());
        assert_eq!(
            identity_manager.key_bundle_status(&key_manager_y).unwrap(),
            MemberStatus::Healthy(bundle.clone())
        );
    }

    #[test]
    fn process_key_bundle_registers_member() {
        let alice_rng = Rng::from_seed([1; 32]);
        let alice_credentials = Credentials::from_rng(&alice_rng).unwrap();
        let alice_identity_manager = TestIdentityManager::new(Config::default());
        let (_, alice_key_registry_y) = init_keys(&alice_credentials);

        let bob_rng = Rng::from_seed([2; 32]);
        let bob_credentials = Credentials::from_rng(&bob_rng).unwrap();
        let bob_identity_manager = TestIdentityManager::new(Config::default());
        let (bob_key_manager_y, bob_key_registry_y) = init_keys(&bob_credentials);
        let bob_id = bob_credentials.verifying_key();

        let (_, _, bob_member) = bob_identity_manager
            .rotate_key_bundle(
                bob_key_manager_y,
                bob_key_registry_y,
                &bob_credentials,
                &bob_rng,
            )
            .unwrap();
        let (key_registry_y, _event) = alice_identity_manager
            .process(alice_key_registry_y, &bob_member)
            .unwrap();

        let (_, bundle): (_, Option<LongTermKeyBundle>) =
            KeyRegistry::key_bundle(key_registry_y, &bob_id).unwrap();
        assert!(bundle.is_some());
        let bundle_identity_key = bundle.unwrap().identity_key().to_owned();
        assert_eq!(
            bundle_identity_key,
            bob_credentials.identity_secret().verifying_key().unwrap()
        );
    }

    #[test]
    fn rotation_due_when_about_to_expire() {
        let rng = Rng::from_seed([1; 32]);
        let credentials = Credentials::from_rng(&rng).unwrap();
        let mut alice_identity_manager = TestIdentityManager::new(Config::default());

        let (key_manager_y, key_registry_y) = init_keys(&credentials);
        let (key_manager_y, key_registry_y, alice_1) = alice_identity_manager
            .rotate_key_bundle(key_manager_y, key_registry_y, &credentials, &rng)
            .unwrap();
        let bundle_1 = alice_1.key_bundle().clone();

        // Override max. lifetime of 90 days (default) with pre-rotation window to force rotation.
        //
        // TODO: Use Duration::from_days() once stabilized:
        // https://github.com/rust-lang/rust/issues/120301
        alice_identity_manager.config.pre_key_rotate_after = Duration::from_hours(24 * 1024);

        // Key bundle is still valid, but we want to pessimistically already distribute a new one.
        assert_eq!(
            alice_identity_manager
                .key_bundle_status(&key_manager_y)
                .unwrap(),
            MemberStatus::RotationDue(bundle_1.clone())
        );

        // Make lifetime of next key longer to "win" over the previous one, in case it is still
        // considered valid due to a race condition (both keys can be generated "at the same time").
        //
        // TODO: Use Duration::from_days() once stabilized:
        // https://github.com/rust-lang/rust/issues/120301
        alice_identity_manager.config.pre_key_lifetime = Duration::from_secs(60 * 60 * 24 * 2048);

        let (key_manager_y, _, alice_2) = alice_identity_manager
            .rotate_key_bundle(key_manager_y, key_registry_y, &credentials, &rng)
            .unwrap();
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

        assert_eq!(
            alice_identity_manager
                .key_bundle_status(&key_manager_y)
                .unwrap(),
            MemberStatus::Healthy(bundle_2)
        );
    }
}
