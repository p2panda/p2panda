// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_auth::traits::Resolver;
use p2panda_encryption::Rng;
use p2panda_encryption::key_manager::{KeyManager, KeyManagerError};
use p2panda_encryption::key_registry::KeyRegistry;
use p2panda_encryption::traits::PreKeyManager;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::auth::orderer::AuthOrderer;
use crate::forge::Forge;
use crate::member::Member;
use crate::message::{AuthoredMessage, SpacesArgs, SpacesMessage};
use crate::space::{Space, SpaceError};
use crate::store::{KeyStore, SpaceStore};
use crate::types::{ActorId, AuthDummyStore, Conditions, OperationId};

// Create and manage spaces and groups.
//
// Takes care of ingesting operations, updating spaces, groups and member key-material. Has access
// to the operation and group stores, orderer, key-registry and key-manager.
//
// Routes operations to the correct space(s), group(s) or member.
//
// Only one instance of `Spaces` per app user.
//
// Operations are created and published within the spaces service, reacting to arriving
// operations, due to api calls (create group, create space), or triggered by key-bundles
// expiring.
//
// Users of spaces can subscribe to events which inform about member, group or space state
// changes, application data being decrypted, pre-key bundles being published, we were added or
// removed from a space.
//
// Is agnostic to current p2panda-streams, networking layer, data type.
#[derive(Debug)]
pub struct Manager<S, F, M, C, RS> {
    #[allow(clippy::type_complexity)]
    pub(crate) inner: Arc<RwLock<ManagerInner<S, F, M, C, RS>>>,
}

#[derive(Debug)]
pub(crate) struct ManagerInner<S, F, M, C, RS> {
    pub(crate) store: S,
    pub(crate) forge: F,
    pub(crate) rng: Rng,
    _marker: PhantomData<(M, C, RS)>,
}

impl<S, F, M, C, RS> Manager<S, F, M, C, RS>
where
    S: SpaceStore<M, C, RS> + KeyStore,
    F: Forge<M, C>,
    M: AuthoredMessage + SpacesMessage<C>,
    C: Conditions,
    // @TODO: Can we get rid of this Debug requirement here?
    RS: Debug + Resolver<ActorId, OperationId, C, AuthOrderer, AuthDummyStore>,
{
    #[allow(clippy::result_large_err)]
    pub fn new(store: S, forge: F, rng: Rng) -> Result<Self, ManagerError<S, F, M, C, RS>> {
        let inner = ManagerInner {
            store,
            forge,
            rng,
            _marker: PhantomData,
        };

        Ok(Self {
            inner: Arc::new(RwLock::new(inner)),
        })
    }

    pub async fn space(
        &self,
        id: &ActorId,
    ) -> Result<Option<Space<S, F, M, C, RS>>, ManagerError<S, F, M, C, RS>> {
        let has_space = {
            let inner = self.inner.read().await;
            inner
                .store
                .has_space(id)
                .await
                .map_err(ManagerError::SpaceStore)?
        };

        if has_space {
            Ok(Some(Space::new(self.clone(), *id)))
        } else {
            Ok(None)
        }
    }

    #[allow(clippy::type_complexity, clippy::result_large_err)]
    pub async fn create_space(
        &self,
        initial_members: &[(ActorId, Access<C>)],
    ) -> Result<(Space<S, F, M, C, RS>, M), ManagerError<S, F, M, C, RS>> {
        // @TODO: Check if initial members are known and have a key bundle present, throw error
        // otherwise.

        // @TODO: Assign GroupMember type to every actor based on looking up our own state,
        // checking if actor is a group or individual.

        // @TODO: Throw error when user tries to add a space to a space.

        let initial_members = initial_members
            .iter()
            .map(|(actor, access)| (GroupMember::Individual(actor.to_owned()), access.to_owned()))
            .collect();

        let (space, message) = Space::create(self.clone(), initial_members)
            .await
            .map_err(ManagerError::Space)?;

        Ok((space, message))
    }

    pub async fn id(&self) -> ActorId {
        let inner = self.inner.read().await;
        inner.forge.public_key().into()
    }

    pub async fn me(&self) -> Result<Member, ManagerError<S, F, M, C, RS>> {
        let inner = self.inner.read().await;

        let y = inner
            .store
            .key_manager()
            .await
            .map_err(ManagerError::KeyStore)?;

        // @TODO: What happens if the forge changes their private key?
        let my_id = inner.forge.public_key().into();

        Ok(Member::new(my_id, KeyManager::prekey_bundle(&y)))
    }

    pub async fn register_member(
        &mut self,
        member: &Member,
    ) -> Result<(), ManagerError<S, F, M, C, RS>> {
        // @TODO: Reject invalid / expired key bundles.

        let mut inner = self.inner.write().await;

        let y = inner
            .store
            .key_registry()
            .await
            .map_err(ManagerError::KeyStore)?;

        // @TODO: Setting longterm bundle should overwrite previous one if this is newer.
        let y_ii = KeyRegistry::add_longterm_bundle(y, member.id(), member.key_bundle().clone());

        inner
            .store
            .set_key_registry(&y_ii)
            .await
            .map_err(ManagerError::KeyStore)?;

        Ok(())
    }

    // We expect messages to be signature-checked, dependency-checked & partially ordered here.
    pub async fn process(&mut self, message: &M) -> Result<(), ManagerError<S, F, M, C, RS>> {
        // Route message to the regarding member-, group- or space processor.
        match message.args() {
            // Received key bundle from a member.
            SpacesArgs::KeyBundle {} => {
                // @TODO:
                // - Check if it is valid
                // - Store it in key manager if it is newer than our previously stored one (if given)
                todo!()
            }
            // Received control message related to a group or space.
            SpacesArgs::ControlMessage {
                id,
                control_message,
                ..
            } => {
                // @TODO:
                // - Detect if id is related to a space or group.
                // - Also process group messages.

                // @TODO: Make sure claimed "group member" types in control messages are correct.

                let mut space = match self.space(id).await? {
                    Some(space) => space,
                    None => {
                        if !control_message.is_create() {
                            // If this is not a "create" message we should have learned about the space
                            // before. This can be either a faulty message or a problem with the message
                            // orderer.
                            return Err(ManagerError::UnexpectedMessage(message.id()));
                        }

                        // @TODO: This is a bit strange. What are the API guarantees here over
                        // "inexistant" spaces. We should tell from the outside that a new one is
                        // initialised instead of pointing at an existing one.
                        Space::new(self.clone(), *id)
                    }
                };

                space.process(message).await.map_err(ManagerError::Space)?;
            }
            // Received encrypted application data for a space.
            SpacesArgs::Application { space_id, .. } => {
                let Some(mut space) = self.space(space_id).await? else {
                    return Err(ManagerError::UnexpectedMessage(message.id()));
                };

                space.process(message).await.map_err(ManagerError::Space)?;
            }
        }

        // @TODO: Return events.

        Ok(())
    }
}

// Deriving clone on Manager will enforce generics to also impl Clone even though we are wrapping
// them in an Arc. Related: https://stackoverflow.com/questions/72150623
impl<S, F, M, C, RS> Clone for Manager<S, F, M, C, RS> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum ManagerError<S, F, M, C, RS>
where
    S: SpaceStore<M, C, RS> + KeyStore,
    F: Forge<M, C>,
    C: Conditions,
    RS: Resolver<ActorId, OperationId, C, AuthOrderer, AuthDummyStore>,
{
    #[error(transparent)]
    Space(#[from] SpaceError<S, F, M, C, RS>),

    #[error(transparent)]
    KeyManager(#[from] KeyManagerError),

    #[error("{0}")]
    KeyStore(<S as KeyStore>::Error),

    #[error("{0}")]
    SpaceStore(<S as SpaceStore<M, C, RS>>::Error),

    #[error("received unexpected message with id {0}, maybe it arrived out-of-order")]
    UnexpectedMessage(OperationId),
}
