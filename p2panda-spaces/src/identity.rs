// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;

use p2panda_auth::traits::{Conditions};
use p2panda_encryption::key_bundle::{KeyBundleError, LongTermKeyBundle};
use p2panda_encryption::key_manager::{KeyManager, KeyManagerError};
use p2panda_encryption::key_registry::KeyRegistry;
use p2panda_encryption::traits::{KeyBundle, PreKeyManager};
use thiserror::Error;

use crate::forge::Forge;
use crate::manager::Manager;
use crate::member::Member;
use crate::message::{AuthoredMessage, SpacesArgs, SpacesMessage};
use crate::store::{AuthStore, KeyStore, MessageStore, SpaceStore};
use crate::traits::SpaceId;
use crate::types::{ActorId, AuthResolver, OperationId};
use crate::utils::now;

pub struct Identity<ID, S, F, M, C, RS> {
    _phantom: PhantomData<(ID, S, F, M, C, RS)>,
}

impl<ID, S, F, M, C, RS> Identity<ID, S, F, M, C, RS>
where
    ID: SpaceId,
    S: SpaceStore<ID, M, C> + KeyStore + AuthStore<C> + MessageStore<M> + Debug,
    F: Forge<ID, M, C> + Debug,
    M: AuthoredMessage + SpacesMessage<ID, C>,
    C: Conditions,
    RS: Debug + AuthResolver<C>,
{
    /// The public key of the local actor.
    pub(crate) async fn id(manager_ref: Manager<ID, S, F, M, C, RS>) -> ActorId {
        let inner = manager_ref.inner.read().await;
        inner.forge.public_key().into()
    }

    /// The local actor id and their long-term key bundle.
    /// 
    /// Note: key bundle will be rotated if the latest is reaching it's configured expiry date.
    pub(crate) async fn me(
        manager_ref: Manager<ID, S, F, M, C, RS>,
    ) -> Result<Member, IdentityError<ID, S, F, M, C>> {
        let my_id = Self::id(manager_ref.clone()).await;
        let mut manager = manager_ref.inner.write().await;

        let key_manager_y = manager
            .store
            .key_manager()
            .await
            .map_err(IdentityError::KeyStore)?;

        // Automatically rotate pre key when it reached critical expiry date.
        let key_bundle = if now() - manager.my_keys_rotated_at
            > manager.config.pre_key_rotate_after.as_secs()
        {
            manager.my_keys_rotated_at = now();

            // This mutates the state internally.
            let key_manager_y_i =
                KeyManager::rotate_prekey(key_manager_y, manager.config.lifetime(), &manager.rng)?;

            let key_registry_y = manager
                .store
                .key_registry()
                .await
                .map_err(IdentityError::KeyStore)?;

            // Register our own key bundle.
            let key_bundle = KeyManager::prekey_bundle(&key_manager_y_i);
            let key_registry_y_i =
                KeyRegistry::add_longterm_bundle(key_registry_y, my_id, key_bundle.clone());

            manager
                .store
                .set_key_manager(&key_manager_y_i)
                .await
                .map_err(IdentityError::KeyStore)?;
            manager
                .store
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
        manager_ref: Manager<ID, S, F, M, C, RS>,
        member: &Member,
    ) -> Result<(), IdentityError<ID, S, F, M, C>> {
        member.key_bundle().verify()?;

        let mut manager = manager_ref.inner.write().await;

        let y = manager
            .store
            .key_registry()
            .await
            .map_err(IdentityError::KeyStore)?;

        // @TODO: Setting longterm bundle should overwrite previous one if this is newer.
        let y_ii = KeyRegistry::add_longterm_bundle(y, member.id(), member.key_bundle().clone());

        manager
            .store
            .set_key_registry(&y_ii)
            .await
            .map_err(IdentityError::KeyStore)?;

        Ok(())
    }

    /// Check if my latest key bundle has expired.
    pub async fn key_bundle_expired(manager_ref: Manager<ID, S, F, M, C, RS>) -> bool {
        let inner = manager_ref.inner.read().await;
        now() - inner.my_keys_rotated_at > inner.config.pre_key_rotate_after.as_secs()
    }

    /// Forge a key bundle message containing my latest key bundle.
    /// 
    /// Note: key bundle will be rotated if the latest is reaching it's configured expiry date.
    pub async fn key_bundle(
        manager_ref: Manager<ID, S, F, M, C, RS>,
    ) -> Result<M, IdentityError<ID, S, F, M, C>> {
        let me = Self::me(manager_ref.clone()).await?;
        let args = SpacesArgs::KeyBundle {
            key_bundle: me.key_bundle().clone(),
        };
        let mut inner = manager_ref.inner.write().await;
        let message = inner
            .forge
            .forge(args)
            .await
            .map_err(IdentityError::Forge)?;
        Ok(message)
    }

    /// Process a key bundle received from the network. 
    pub async fn process_key_bundle(
        manager_ref: Manager<ID, S, F, M, C, RS>,
        author: ActorId,
        key_bundle: &LongTermKeyBundle,
    ) -> Result<(), IdentityError<ID, S, F, M, C>> {
        key_bundle.verify()?;
        let identity_key = ActorId::from_bytes(key_bundle.identity_key().as_bytes())
            .expect("valid public key bytes");
        if identity_key == author {
            return Err(IdentityError::KeyBundleAuthor(
                identity_key,
                author,
            ));
        }

        // @TODO: Setting longterm bundle should overwrite previous one if this is newer.
        let member = Member::new(identity_key, key_bundle.clone());
        Self::register_member(manager_ref.clone(), &member).await?;
        Ok(())
    }
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum IdentityError<ID, S, F, M, C>
where
    ID: SpaceId,
    S: SpaceStore<ID, M, C> + KeyStore + AuthStore<C> + MessageStore<M>,
    F: Forge<ID, M, C>,
    C: Conditions,
{
    #[error("{0}")]
    Forge(F::Error),

    #[error(transparent)]
    KeyManager(#[from] KeyManagerError),

    #[error(transparent)]
    KeyBundle(#[from] KeyBundleError),

    #[error("received long-term key bundle for {0} on message signed by unexpected author {1}")]
    KeyBundleAuthor(ActorId, ActorId),

    #[error("{0}")]
    KeyStore(<S as KeyStore>::Error),

    #[error("{0}")]
    MessageStore(<S as MessageStore<M>>::Error),

    #[error(
        "received space message with id {0} before auth message {1}, maybe it arrived out-of-order"
    )]
    MissingAuthMessage(OperationId, OperationId),
}
