// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_auth::traits::{Conditions, Operation};
use p2panda_encryption::Rng;
use p2panda_encryption::key_manager::{KeyManager, KeyManagerError};
use p2panda_encryption::key_registry::KeyRegistry;
use p2panda_encryption::traits::PreKeyManager;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::auth::message::AuthMessage;
use crate::event::Event;
use crate::forge::Forge;
use crate::group::{Group, GroupError};
use crate::member::Member;
use crate::message::{AuthoredMessage, SpacesArgs, SpacesMessage};
use crate::space::{Space, SpaceError};
use crate::store::{AuthStore, KeyStore, MessageStore, SpaceStore};
use crate::traits::SpaceId;
use crate::types::{ActorId, AuthResolver, OperationId};

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
pub struct Manager<ID, S, F, M, C, RS> {
    #[allow(clippy::type_complexity)]
    pub(crate) inner: Arc<RwLock<ManagerInner<ID, S, F, M, C, RS>>>,
}

#[derive(Debug)]
pub(crate) struct ManagerInner<ID, S, F, M, C, RS> {
    pub(crate) store: S,
    pub(crate) forge: F,
    pub(crate) rng: Rng,
    _marker: PhantomData<(ID, M, C, RS)>,
}

impl<ID, S, F, M, C, RS> Manager<ID, S, F, M, C, RS>
where
    ID: SpaceId,
    // @TODO: make extensions generic.
    S: SpaceStore<ID, M, C> + KeyStore + AuthStore<C> + MessageStore<M>,
    F: Forge<ID, M, C>,
    M: AuthoredMessage + SpacesMessage<ID, C>,
    C: Conditions,
    // @TODO: Can we get rid of this Debug requirement here?
    RS: Debug + AuthResolver<C>,
{
    #[allow(clippy::result_large_err)]
    pub fn new(store: S, forge: F, rng: Rng) -> Result<Self, ManagerError<ID, S, F, M, C, RS>> {
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
        id: ID,
    ) -> Result<Option<Space<ID, S, F, M, C, RS>>, ManagerError<ID, S, F, M, C, RS>> {
        let has_space = {
            let inner = self.inner.read().await;
            inner
                .store
                .has_space(&id)
                .await
                .map_err(ManagerError::SpaceStore)?
        };

        if has_space {
            Ok(Some(Space::new(self.clone(), id)))
        } else {
            Ok(None)
        }
    }

    pub async fn group(
        &self,
        id: ActorId,
    ) -> Result<Option<Group<ID, S, F, M, C, RS>>, ManagerError<ID, S, F, M, C, RS>> {
        let auth_y = {
            let manager = self.inner.read().await;
            manager.store.auth().await.map_err(GroupError::AuthStore)?
        };

        // @TODO: we need members(..) to return an Option which is None when the group is not
        // present in the auth state. Or better yet, introduce a new method which produces a
        // list of all current known actors in the auth context. This check that the group
        // is empty does not account for present but empty groups.
        if auth_y.members(id).is_empty() {
            Ok(None)
        } else {
            Ok(Some(Group::new(self.clone(), id)))
        }
    }

    #[allow(clippy::type_complexity, clippy::result_large_err)]
    pub async fn create_space(
        &self,
        id: ID,
        initial_members: &[(ActorId, Access<C>)],
    ) -> Result<(Space<ID, S, F, M, C, RS>, Vec<M>), ManagerError<ID, S, F, M, C, RS>> {
        // @TODO: Check if initial members are known and have a key bundle present, throw error
        // otherwise.

        let initial_members = {
            let manager = self.inner.read().await;
            let auth_y = manager
                .store
                .auth()
                .await
                .map_err(ManagerError::AuthStore)?;
            let mut initial_members_inner = vec![];

            for (actor, access) in initial_members {
                // @TODO: we need members(..) to return an Option which is None when the group is not
                // present in the auth state. Or better yet, introduce a new method which produces a
                // list of all current known actors in the auth context. This check that the group
                // is empty does not account for present but empty groups.
                if auth_y.members(*actor).is_empty() {
                    initial_members_inner
                        .push((GroupMember::Individual(*actor), access.to_owned()));
                } else {
                    initial_members_inner.push((GroupMember::Group(*actor), access.to_owned()));
                }
            }

            initial_members_inner
        };

        let (space, messages) = Space::create(self.clone(), id, initial_members)
            .await
            .map_err(ManagerError::Space)?;

        Ok((space, messages))
    }

    #[allow(clippy::type_complexity, clippy::result_large_err)]
    pub async fn create_group(
        &self,
        initial_members: &[(ActorId, Access<C>)],
    ) -> Result<(Group<ID, S, F, M, C, RS>, M), ManagerError<ID, S, F, M, C, RS>> {
        // @TODO: Assign GroupMember type to every actor based on looking up our own state,
        // checking if actor is a group or individual.

        let initial_members = {
            let manager = self.inner.read().await;
            let auth_y = manager
                .store
                .auth()
                .await
                .map_err(ManagerError::AuthStore)?;
            let mut initial_members_inner = vec![];

            for (actor, access) in initial_members {
                // @TODO: we need members(..) to return an Option which is None when the group is not
                // present in the auth state. Or better yet, introduce a new method which produces a
                // list of all current known actors in the auth context. This check that the group
                // is empty does not account for present but empty groups.
                if auth_y.members(*actor).is_empty() {
                    initial_members_inner
                        .push((GroupMember::Individual(*actor), access.to_owned()));
                } else {
                    initial_members_inner.push((GroupMember::Group(*actor), access.to_owned()));
                }
            }

            initial_members_inner
        };

        let (group, message) = Group::create(self.clone(), initial_members)
            .await
            .map_err(ManagerError::Group)?;

        Ok((group, message))
    }

    // @TODO: Make it work without async
    pub async fn id(&self) -> ActorId {
        let inner = self.inner.read().await;
        inner.forge.public_key().into()
    }

    pub async fn me(&self) -> Result<Member, ManagerError<ID, S, F, M, C, RS>> {
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
        &self,
        member: &Member,
    ) -> Result<(), ManagerError<ID, S, F, M, C, RS>> {
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
    pub async fn process(
        &self,
        message: &M,
    ) -> Result<Vec<Event<ID>>, ManagerError<ID, S, F, M, C, RS>> {
        // Route message to the regarding member-, group- or space processor.
        let events = match message.args() {
            // Received key bundle from a member.
            SpacesArgs::KeyBundle {} => {
                // @TODO:
                // - Check if it is valid
                // - Store it in key manager if it is newer than our previously stored one (if given)
                todo!()
            }
            SpacesArgs::Auth { .. } => {
                // @TODO: move into own method.
                Group::process(self.clone(), message)
                    .await
                    .map_err(ManagerError::Group)?
            }
            // Received control message related to a group or space.
            SpacesArgs::SpaceMembership {
                space_id,
                auth_message_id,
                ..
            } => {
                // @TODO: move into own method.
                // Get auth message.
                let auth_message = {
                    let inner = self.inner.read().await;
                    let Some(message) = inner
                        .store
                        .message(auth_message_id)
                        .await
                        .map_err(ManagerError::MessageStore)?
                    else {
                        return Err(ManagerError::MissingAuthMessage(
                            message.id(),
                            *auth_message_id,
                        ));
                    };

                    match message.args() {
                        SpacesArgs::Auth { .. } => AuthMessage::from_forged(&message),
                        _ => {
                            return Err(ManagerError::IncorrectMessageVariant(*auth_message_id));
                        }
                    }
                };

                let space = match self.space(*space_id).await? {
                    Some(space) => space,
                    None => {
                        if !auth_message.payload().is_create() {
                            // If this is not a "create" message we should have learned about the space
                            // before. This can be either a faulty message or a problem with the message
                            // orderer.
                            return Err(ManagerError::UnexpectedMessage(message.id()));
                        }

                        // @TODO: This is a bit strange. What are the API guarantees here over
                        // "inexistant" spaces. We should tell from the outside that a new one is
                        // initialised instead of pointing at an existing one.
                        Space::new(self.clone(), *space_id)
                    }
                };

                space
                    .process(message, Some(&auth_message))
                    .await
                    .map_err(ManagerError::Space)?
            }
            SpacesArgs::SpaceUpdate { .. } => unimplemented!(),
            // Received encrypted application data for a space.
            SpacesArgs::Application { space_id, .. } => {
                let Some(space) = self.space(*space_id).await? else {
                    return Err(ManagerError::UnexpectedMessage(message.id()));
                };

                space
                    .process(message, None)
                    .await
                    .map_err(ManagerError::Space)?
            }
        };

        Ok(events)
    }
}

// Deriving clone on Manager will enforce generics to also impl Clone even though we are wrapping
// them in an Arc. Related: https://stackoverflow.com/questions/72150623
impl<ID, S, F, M, C, RS> Clone for Manager<ID, S, F, M, C, RS> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum ManagerError<ID, S, F, M, C, RS>
where
    ID: SpaceId,
    S: SpaceStore<ID, M, C> + KeyStore + AuthStore<C> + MessageStore<M>,
    F: Forge<ID, M, C>,
    C: Conditions,
    RS: Debug + AuthResolver<C>,
{
    #[error(transparent)]
    Space(#[from] SpaceError<ID, S, F, M, C, RS>),

    #[error(transparent)]
    Group(#[from] GroupError<ID, S, F, M, C, RS>),

    #[error(transparent)]
    KeyManager(#[from] KeyManagerError),

    #[error("{0}")]
    KeyStore(<S as KeyStore>::Error),

    #[error("{0}")]
    SpaceStore(<S as SpaceStore<ID, M, C>>::Error),

    #[error("{0}")]
    AuthStore(<S as AuthStore<C>>::Error),

    #[error("{0}")]
    MessageStore(<S as MessageStore<M>>::Error),

    #[error("received unexpected message with id {0}, maybe it arrived out-of-order")]
    UnexpectedMessage(OperationId),

    #[error(
        "received space message with id {0} before auth message {1}, maybe it arrived out-of-order"
    )]
    MissingAuthMessage(OperationId, OperationId),

    #[error("unexpected message variant, expected auth {0}")]
    IncorrectMessageVariant(OperationId),
}
