// SPDX-License-Identifier: MIT OR Apache-2.0

//! High-level API for managing spaces, groups and member keys.
use std::borrow::Borrow;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use p2panda_auth::Access;
use p2panda_auth::traits::{Conditions, Operation};
use p2panda_core::traits::{Digest, Provenance, ShortFormat};
use p2panda_core::{Hash, SigningKey, VerifyingKey};
use p2panda_encryption::{Rng, RngError};
use p2panda_store::Transaction;
use p2panda_store::groups::GroupsStore;
use p2panda_store::key_registry::KeyRegistryStore;
use p2panda_store::key_secrets::KeySecretsStore;
use p2panda_store::spaces::{SpacesMessageStore, SpacesStore};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::debug;

use crate::auth::message::AuthMessage;
use crate::event::Event;
use crate::forge::Forge;
use crate::group::{Group, GroupError};
use crate::identity::{IdentityError, IdentityManager};
use crate::member::Member;
use crate::message::{SpaceMembershipMessage, SpacesArgs, SpacesMessage};
use crate::space::{GroupsScope, Space, SpaceError, SpacesState};
use crate::store::SpacesStoreState;
use crate::types::AuthGroupState;
use crate::{ActorId, Config, Credentials, GroupId, SpaceId};

/// Identifier used to store groups state into database.
pub const GLOBAL_GROUPS_CONTEXT_ID: &[u8] = b"global-groups-context";

/// API for creating and managing groups and spaces.
///
/// There should be only one manager instance per application. Any messages received from other
/// instances must be processed on the manager. All methods which mutate state locally return M
/// message(s) which must be replicated to and processed by other instances. Any action which
/// mutates state (both local method calls and processed messages) will emit events which can be
/// sent to any higher levels to inform of any state changes.
///
/// In order to add an actor to a space, we first need to have a key bundle generated from a
/// not-expired pre-key. The manager offers an API for checking our latest key-bundle is valid and
/// issuing new key bundles to be replicated with other instances.
///
/// All methods are idempotent; messages can be processed multiple times without causing any
/// additional state changes.
///
/// p2panda-spaces is agnostic to the concrete message type used to send control and application
/// messages, providing the trait requirements are met.
///
/// ## Requirements
///
/// All messages must be ordered according to their causal relationship _before_ being processed
/// on the manager. All messages created within p2panda-spaces express their dependencies; these
/// should be used to perform partial ordering of all incoming messages.
#[derive(Debug)]
pub struct Manager<S, F, C> {
    pub(crate) actor_id: ActorId,
    #[allow(clippy::type_complexity)]
    pub(crate) inner: Arc<RwLock<ManagerInner<S, F, C>>>,
}

#[derive(Debug)]
pub(crate) struct ManagerInner<S, F, C> {
    pub store: S,
    pub(crate) identity: IdentityManager<S, F, C>,
    pub(crate) rng: Rng,
    _marker: PhantomData<F>,
}

impl<S, F, C> Manager<S, F, C>
where
    S: Clone
        + SpacesStore<SpacesStoreState<C>>
        + SpacesMessageStore<SpacesArgs<C>>
        + GroupsStore<AuthMessage<C>, C>
        + KeyRegistryStore
        + KeySecretsStore
        + Transaction,
    F: Forge<C>,
    C: Conditions,
{
    /// Instantiate a new manager.
    #[allow(clippy::result_large_err)]
    pub fn new(
        store: S,
        forge: F,
        credentials: Credentials,
        rng: Rng,
    ) -> Result<Self, ManagerError<F, C>> {
        Self::new_with_config(store, forge, credentials, Config::default(), rng)
    }

    /// Instantiate a new manager with custom configuration.
    #[allow(clippy::result_large_err)]
    pub fn new_with_config(
        store: S,
        forge: F,
        credentials: Credentials,
        config: Config,
        rng: Rng,
    ) -> Result<Self, ManagerError<F, C>> {
        let actor_id: ActorId = credentials.verifying_key();
        let identity = IdentityManager::new(store.clone(), forge, credentials, config, &rng)?;
        let inner = ManagerInner {
            store,
            identity,
            rng,
            _marker: PhantomData,
        };
        Ok(Self {
            actor_id,
            inner: Arc::new(RwLock::new(inner)),
        })
    }

    /// Get a space by id.
    ///
    /// A space instance provides an API for adding and removing members from the space and
    /// querying the current space members.
    pub async fn space(
        &self,
        id: impl Into<Hash>,
    ) -> Result<Option<Space<S, F, C>>, ManagerError<F, C>> {
        let id = id.into();

        let has_space = {
            let manager = self.inner.read().await;
            manager
                .store
                .has_space(&id)
                .await
                .map_err(|err| StoreError::SpacesStore(err.to_string()))?
        };

        if has_space {
            Ok(Some(Space::new(self.clone(), id)))
        } else {
            Ok(None)
        }
    }

    /// Get a group by id.
    ///
    /// A group instance provides an API for adding and removing members from a group and querying
    /// the current group members.
    pub async fn group(
        &self,
        id: impl Into<GroupId>,
    ) -> Result<Option<Group<S, F, C>>, ManagerError<F, C>> {
        let id = id.into();
        let groups_y = self.get_groups_state().await?;

        // Check if this group exists in the auth state.
        if groups_y.has_group(id) {
            Ok(Some(Group::new(self.clone(), id)))
        } else {
            Ok(None)
        }
    }

    /// Create a new space containing initial members and access levels.
    ///
    /// If not already included, then the local actor (creator of this space) will be added to the
    /// initial members and given manage access level.
    ///
    /// Returns resulting auth and space state and messages for processing.
    pub async fn create_space(
        &self,
        id: impl Into<SpaceId>,
        initial_members: &[(ActorId, Access<C>)],
    ) -> Result<
        (
            AuthGroupState<C>,
            SpacesState<C>,
            Vec<F::Message>,
            Vec<Event<C>>,
        ),
        ManagerError<F, C>,
    > {
        let id = id.into();

        let (groups_y, space_y, messages, events) =
            Space::create(self.clone(), id, initial_members.to_owned())
                .await
                .map_err(ManagerError::Space)?;

        Ok((groups_y, space_y, messages, events))
    }

    /// Create a new group containing initial members with associated access levels.
    ///
    /// It is possible to create a group where the creator is not an initial member or is a member
    /// without manager rights. If this is done then after creation no further change of the group
    /// membership would be possible.
    ///
    /// Returns resulting auth state, group id and message for processing.
    pub async fn create_group(
        &self,
        initial_members: &[(ActorId, Access<C>)],
    ) -> Result<(AuthGroupState<C>, GroupId, F::Message, Event<C>), ManagerError<F, C>> {
        let groups_y = self.get_groups_state().await?;

        // Generate random group id.
        let group_id: GroupId = {
            let manager = self.inner.read().await;
            let signing_key = SigningKey::from_bytes(&manager.rng.random_array()?);
            signing_key.verifying_key()
        };

        let (groups_y, message, event) =
            Group::create(self.clone(), groups_y, group_id, initial_members.to_owned())
                .await
                .map_err(ManagerError::Group)?;

        Ok((groups_y, group_id, message, event))
    }

    /// Process a spaces message.
    ///
    /// We expect messages to be signature-checked, dependency-checked & partially ordered.
    ///
    /// Returns events which inform users of any state changes which occurred.
    pub async fn process<M>(
        &self,
        message: &M,
    ) -> Result<
        (
            Option<AuthGroupState<C>>,
            Option<SpacesState<C>>,
            Vec<Event<C>>,
        ),
        ManagerError<F, C>,
    >
    where
        M: Provenance<VerifyingKey> + Digest<Hash> + Borrow<SpacesArgs<C>>,
    {
        let args = message.borrow();
        let span = tracing::debug_span!("spaces", node_id = self.id().fmt_short());
        let _guard = span.enter();

        debug!(
            message_id = message.hash().fmt_short(),
            author = message.author().fmt_short(),
            variant = args.variant_str(),
            "process message"
        );

        // Route message to the regarding member-, group- or space processor.
        let result = match args {
            // Received key bundle from a member.
            SpacesArgs::Member(member) => {
                let mut manager = self.inner.write().await;
                let event = manager
                    .identity
                    .process(member)
                    .await
                    .map_err(ManagerError::IdentityManager)?;

                (None, None, vec![event])
            }
            SpacesArgs::Group { .. } => {
                let event = Group::process(self.clone(), &SpacesMessage::auth(message))
                    .await
                    .map_err(ManagerError::Group)?;

                if let Some((groups_y, event)) = event {
                    (Some(groups_y), None, vec![event])
                } else {
                    (None, None, vec![])
                }
            }
            // Received control message related to a space.
            SpacesArgs::SpaceMembership { space_id, .. } => {
                if let Some((space_y, events)) = self
                    .handle_space_membership_message(
                        *space_id,
                        &SpacesMessage::space_membership(message),
                    )
                    .await?
                {
                    (None, Some(space_y), events)
                } else {
                    (None, None, vec![])
                }
            }
            SpacesArgs::SpaceUpdate { .. } => unimplemented!(),
            // Received encrypted application data for a space.
            SpacesArgs::Application { space_id, .. } => {
                let Some(space) = self.space(*space_id).await? else {
                    return Err(ManagerError::UnexpectedMessage(message.hash()));
                };

                if let Some((space_y, events)) = space
                    .handle_application_message(&SpacesMessage::application(message))
                    .await
                    .map_err(ManagerError::Space)?
                {
                    (None, Some(space_y), events)
                } else {
                    (None, None, vec![])
                }
            }
        };

        Ok(result)
    }

    /// The public key of the local actor.
    pub fn id(&self) -> ActorId {
        self.actor_id
    }

    /// The local actor id and their long-term key bundle.
    ///
    /// Note: Key bundle will be rotated if the latest is reaching it's configured expiry date.
    pub async fn me(&self) -> Result<Member, ManagerError<F, C>> {
        let manager = self.inner.write().await;
        manager
            .identity
            .me()
            .await
            .map_err(ManagerError::IdentityManager)
    }

    /// Register a member with long-term key bundle material which was provided through another
    /// channel (QR code scan etc.).
    pub async fn register_member(&self, member: &Member) -> Result<(), ManagerError<F, C>> {
        let mut manager = self.inner.write().await;
        let _event = manager
            .identity
            .process(member)
            .await
            .map_err(ManagerError::IdentityManager);
        Ok(())
    }

    /// Check if my latest key bundle has expired.
    ///
    /// If `true` then users should rotate their pre-key and generate a new bundle message (which
    /// should then be published) by calling `key_bundle_message`.
    pub async fn key_bundle_expired(&self) -> Result<bool, ManagerError<F, C>> {
        let manager = self.inner.read().await;
        Ok(manager.identity.key_bundle_expired().await?)
    }

    /// Forge a key bundle message containing my latest key bundle.
    ///
    /// Note: Key bundle will be rotated if the latest is reaching it's configured expiry date.
    pub async fn key_bundle_message(&self) -> Result<F::Message, ManagerError<F, C>> {
        let mut manager = self.inner.write().await;
        manager
            .identity
            .key_bundle_message()
            .await
            .map_err(ManagerError::IdentityManager)
    }

    /// Get the global auth state.
    pub(crate) async fn get_groups_state(&self) -> Result<AuthGroupState<C>, StoreError> {
        let manager = self.inner.read().await;

        let permit = manager
            .store
            .begin()
            .await
            .map_err(|err| StoreError::Transaction(err.to_string()))?;

        let y = manager
            .store
            .get_groups_state_tx(Hash::digest(GLOBAL_GROUPS_CONTEXT_ID))
            .await
            .map_err(|err| StoreError::GroupsStore(err.to_string()))?;

        manager
            .store
            .commit(permit)
            .await
            .map_err(|err| StoreError::Transaction(err.to_string()))?;

        Ok(y.unwrap_or_default())
    }

    /// Get space state.
    pub(crate) async fn get_space_state(
        &self,
        id: &SpaceId,
    ) -> Result<Option<SpacesStoreState<C>>, StoreError> {
        let manager = self.inner.write().await;

        let permit = manager
            .store
            .begin()
            .await
            .map_err(|err| StoreError::Transaction(err.to_string()))?;

        let y = manager
            .store
            .get_space_state_tx(id)
            .await
            .map_err(|err| StoreError::SpacesStore(err.to_string()))?;

        manager
            .store
            .commit(permit)
            .await
            .map_err(|err| StoreError::Transaction(err.to_string()))?;

        Ok(y)
    }

    /// Return `true` if the space is missing operations from the global groups state filtered to
    /// the passed [`GroupsScope`].
    pub async fn space_repair_required(
        &self,
        space_id: SpaceId,
        scope: &GroupsScope,
    ) -> Result<bool, ManagerError<F, C>> {
        let Some(space_y) = self.get_space_state(&space_id).await? else {
            return Ok(false);
        };
        let groups_y = self.get_groups_state().await?;

        let group_ids = match scope {
            GroupsScope::Global => groups_y.groups_global(),
            GroupsScope::Group(id) => vec![*id],
            GroupsScope::Space => vec![space_y.group_id],
        };

        let space_heads = space_y.groups_y.inner.heads(&group_ids);
        let global_heads = groups_y.inner.heads(&group_ids);

        Ok(space_heads != global_heads)
    }

    async fn handle_space_membership_message(
        &self,
        space_id: SpaceId,
        message: &SpaceMembershipMessage,
    ) -> Result<Option<(SpacesState<C>, Vec<Event<C>>)>, ManagerError<F, C>> {
        // Get auth message.
        let auth_message = {
            let inner = self.inner.read().await;
            let auth_message_id = message.auth_message_id;
            let Some(message) = inner
                .store
                .get_spaces_message(&auth_message_id)
                .await
                .map_err(|err| StoreError::SpacesStore(err.to_string()))?
            else {
                return Err(ManagerError::MissingAuthMessage(
                    message.id,
                    auth_message_id,
                ));
            };

            match message.borrow() {
                SpacesArgs::Group { .. } => SpacesMessage::auth(&message),
                _ => {
                    return Err(ManagerError::IncorrectMessageVariant(auth_message_id));
                }
            }
        };

        let space = match self.space(space_id).await? {
            Some(space) => space,
            None => {
                if !auth_message.action().is_create() {
                    // If this is not a "create" message we should have learned about the space
                    // before. This can be either a faulty message or a problem with the message
                    // orderer.
                    return Err(ManagerError::UnexpectedMessage(message.id));
                }

                // @TODO: This is a bit strange. What are the API guarantees here over
                // "inexistant" spaces. We should tell from the outside that a new one is
                // initialised instead of pointing at an existing one.
                Space::new(self.clone(), space_id)
            }
        };

        space
            .handle_membership_message(message, &auth_message)
            .await
            .map_err(ManagerError::Space)
    }
}

#[cfg(any(test, feature = "test_utils"))]
impl<S, F, C> Manager<S, F, C>
where
    S: Clone
        + SpacesStore<SpacesStoreState<C>>
        + SpacesMessageStore<SpacesArgs<C>>
        + GroupsStore<AuthMessage<C>, C>
        + KeyRegistryStore
        + KeySecretsStore
        + Transaction,
    F: Forge<C>,
    C: Conditions,
{
    /// Create a new group containing initial members with associated access levels.
    ///
    /// Persists resulting state, returns group instance and forged message.
    pub async fn create_group_persisted(
        &self,
        initial_members: &[(ActorId, Access<C>)],
    ) -> Result<(Group<S, F, C>, F::Message, Event<C>), ManagerError<F, C>> {
        let (groups_y, group_id, message, events) = self.create_group(initial_members).await?;
        self.set_groups_state(&groups_y).await?;
        let group = Group::new(self.clone(), group_id);
        Ok((group, message, events))
    }

    /// Create a new space containing initial members and access levels.
    ///
    /// If not already included, then the local actor (creator of this space) will be added to the
    /// initial members and given manage access level.
    ///
    /// Persists resulting state, returns space instance and forged message.
    pub async fn create_space_persisted(
        &self,
        id: SpaceId,
        initial_members: &[(ActorId, Access<C>)],
    ) -> Result<(Space<S, F, C>, Vec<F::Message>, Vec<Event<C>>), ManagerError<F, C>> {
        let (groups_y, space_y, messages, events) = self.create_space(id, initial_members).await?;
        let space_id = space_y.space_id;

        self.set_groups_state(&groups_y).await?;
        self.set_space_state(&space_id, &space_y.into())
            .await
            .map_err(|err| StoreError::SpacesStore(err.to_string()))?;
        let space = Space::new(self.clone(), space_id);

        Ok((space, messages, events))
    }

    /// Set the global auth state.
    pub async fn set_groups_state(&self, y: &AuthGroupState<C>) -> Result<(), StoreError> {
        let manager = self.inner.write().await;

        let permit = manager
            .store
            .begin()
            .await
            .map_err(|err| StoreError::Transaction(err.to_string()))?;

        manager
            .store
            .set_groups_state_tx(Hash::digest(GLOBAL_GROUPS_CONTEXT_ID), y)
            .await
            .map_err(|err| StoreError::GroupsStore(err.to_string()))?;

        manager
            .store
            .commit(permit)
            .await
            .map_err(|err| StoreError::Transaction(err.to_string()))?;

        Ok(())
    }

    /// Persist spaces state to store.
    pub async fn set_space_state(
        &self,
        space_id: &SpaceId,
        y: &SpacesStoreState<C>,
    ) -> Result<(), StoreError> {
        let manager = self.inner.write().await;

        let permit = manager
            .store
            .begin()
            .await
            .map_err(|err| StoreError::Transaction(err.to_string()))?;

        manager
            .store
            .set_space_state_tx(space_id, y)
            .await
            .map_err(|err| StoreError::GroupsStore(err.to_string()))?;

        manager
            .store
            .commit(permit)
            .await
            .map_err(|err| StoreError::Transaction(err.to_string()))?;

        Ok(())
    }

    pub async fn process_persisted<M>(
        &self,
        message: &M,
    ) -> Result<Vec<Event<C>>, ManagerError<F, C>>
    where
        M: Provenance<VerifyingKey> + Digest<Hash> + Borrow<SpacesArgs<C>> + Debug,
    {
        let (groups_y, space_y, events) = self.process(message).await?;

        if let Some(groups_y) = groups_y {
            self.set_groups_state(&groups_y).await?;
        };

        if let Some(space_y) = space_y {
            let space_id = space_y.space_id;
            self.set_space_state(&space_id, &space_y.into()).await?;
        };

        Ok(events)
    }
}

// Deriving clone on Manager will enforce generics to also impl Clone even though we are wrapping
// them in an Arc. Related: https://stackoverflow.com/questions/72150623
impl<S, F, C> Clone for Manager<S, F, C> {
    fn clone(&self) -> Self {
        Self {
            actor_id: self.actor_id,
            inner: self.inner.clone(),
        }
    }
}

/// Errors which can be returned from stores.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("spaces store error: {0}")]
    SpacesStore(String),

    #[error("groups store error: {0}")]
    GroupsStore(String),

    #[error("spaces message store error: {0}")]
    MessageStore(String),

    #[error("key registry store error: {0}")]
    KeyRegistryStore(String),

    #[error("key secret store error: {0}")]
    KeySecretStore(String),

    #[error("store transaction error: {0}")]
    Transaction(String),
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum ManagerError<F, C>
where
    F: Forge<C>,
    C: Conditions,
{
    #[error(transparent)]
    Space(#[from] SpaceError<F, C>),

    #[error(transparent)]
    Group(#[from] GroupError<F, C>),

    #[error(transparent)]
    Store(#[from] StoreError),

    #[error(transparent)]
    IdentityManager(#[from] IdentityError<F, C>),

    #[error("received unexpected message with id {0}, maybe it arrived out-of-order")]
    UnexpectedMessage(Hash),

    #[error(
        "received space message with id {0} before auth message {1}, maybe it arrived out-of-order"
    )]
    MissingAuthMessage(Hash, Hash),

    #[error("space not found {0}")]
    SpaceNotFound(SpaceId),

    #[error("unexpected message variant, expected auth {0}")]
    IncorrectMessageVariant(Hash),

    #[error(transparent)]
    Rng(#[from] RngError),
}
