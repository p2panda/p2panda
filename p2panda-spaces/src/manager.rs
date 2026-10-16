// SPDX-License-Identifier: MIT OR Apache-2.0

//! High-level API for managing spaces, groups and member keys.
use std::borrow::Borrow;
use std::fmt::Debug;
use std::sync::Arc;

use p2panda_auth::Access;
use p2panda_auth::traits::{Conditions, Operation};
use p2panda_core::traits::{Digest, Provenance, ShortFormat};
use p2panda_core::{Hash, SigningKey, VerifyingKey};
use p2panda_encryption::key_bundle::LongTermKeyBundle;
use p2panda_encryption::key_registry::{KeyRegistry, KeyRegistryError};
use p2panda_encryption::traits::PreKeyRegistry;
use p2panda_encryption::{Rng, RngError};
use p2panda_store::Transaction;
use p2panda_store::groups::GroupsStore;
use p2panda_store::key_registry::KeyRegistryStore;
use p2panda_store::key_secrets::PreKeyBundlesStore;
use p2panda_store::spaces::{SpacesMessageStore, SpacesStore};
use thiserror::Error;
use tracing::debug;

use crate::event::Event;
use crate::forge::Forge;
use crate::group::{Group, GroupError, GroupOutput};
use crate::identity::{IdentityError, IdentityManager};
use crate::member::AuthenticatedMember;
use crate::message::{AuthMessage, SpaceMembershipMessage, SpacesArgs, SpacesMessage};
use crate::space::RepairOutput;
use crate::space::{Space, SpaceError, SpaceOutput};
use crate::store::{
    SpaceState, SpacesStoreState, StateChanges, StateError, StateReader, StateTransaction,
};
use crate::utils::{is_member, is_secret_member, removed_space_members};
use crate::waiters::EventWaiters;
use crate::{
    ActorId, Config, Credentials, GroupId, MemberId, PendingGroup, PendingMember, PendingSpace,
    SpaceId,
};

/// API for creating and managing groups and spaces.
///
/// There should be only one manager instance per application. Any messages received from other
/// instances must be processed on the manager. All methods which mutate state locally return M
/// message(s) which must be replicated to and processed by other instances. Any action which
/// mutates state (both local method calls and processed messages) will emit events which can be
/// sent to any higher levels to inform of any state changes.
///
/// In order to add a member to a space, we first need to have a key bundle generated from a
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
/// All messages must be ordered according to their causal relationship _before_ being processed on
/// the manager. All messages created within p2panda-spaces express their dependencies; these should
/// be used to perform partial ordering of all incoming messages.
#[derive(Debug)]
pub struct Manager<S, F, C> {
    actor_id: ActorId,
    inner: Arc<ManagerInner<S, F, C>>,
}

impl<S, F, C> Clone for Manager<S, F, C> {
    // Deriving clone on Manager will enforce generics to also impl Clone even though we are
    // wrapping them in an Arc. Related: <https://stackoverflow.com/questions/72150623>.
    fn clone(&self) -> Self {
        Self {
            actor_id: self.actor_id,
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug)]
struct ManagerInner<S, F, C> {
    pub config: Config,
    pub forge: F,
    pub store: S,
    pub credentials: Credentials,
    pub rng: Rng,
    pub waiters: EventWaiters<C>,
}

impl<S, F, C> Manager<S, F, C>
where
    S: Clone
        + SpacesStore<SpacesStoreState<C>>
        + SpacesMessageStore<F::Message>
        + GroupsStore<AuthMessage<C>, C>
        + KeyRegistryStore
        + PreKeyBundlesStore
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
        let inner = ManagerInner {
            config,
            store,
            forge,
            credentials,
            rng,
            waiters: EventWaiters::new(),
        };
        Ok(Self {
            actor_id,
            inner: Arc::new(inner),
        })
    }

    /// Begin a transaction to change the space- or group state within an atomic read -> modify ->
    /// write cycle.
    ///
    /// All state needs to be read within this transaction and changed state needs to be committed
    /// after all modifications have been done. Dropping it without commit will roll back the
    /// transaction.
    pub(crate) async fn begin_write(&self) -> Result<StateTransaction<'_, S, F, C>, StoreError> {
        StateTransaction::begin(
            &self.inner.store,
            &self.inner.forge,
            &self.inner.credentials,
            &self.inner.rng,
        )
        .await
    }

    /// Read-only access to the latest committed state, without acquiring a transaction for
    /// performance reasons.
    pub(crate) fn read(&self) -> StateReader<'_, S, C> {
        StateReader::new(&self.inner.store, &self.inner.credentials)
    }

    fn identity(&self) -> IdentityManager<F, C> {
        IdentityManager::new(self.inner.config.clone())
    }

    pub(crate) fn waiters(&self) -> &EventWaiters<C> {
        &self.inner.waiters
    }

    /// Create a new group containing initial members with associated access levels.
    ///
    /// It is possible to create a group where the creator is not an initial member or is a member
    /// without manager rights. If this is done then after creation no further change of the group
    /// membership would be possible.
    ///
    /// Persists resulting state, returns group instance and forged message.
    pub async fn create_group(
        &self,
        initial_members: &[(ActorId, Access<C>)],
    ) -> Result<(Group<S, F, C>, GroupOutput<C, F::Message>), ManagerError<F, C>> {
        let tx = self.begin_write().await?;

        // Read.

        let global_groups_y = tx.global_groups_state().await?;

        // Generate random group id.
        let group_id: GroupId = {
            let signing_key = SigningKey::from_bytes(&tx.ctx.rng.random_array()?);
            signing_key.verifying_key()
        };

        // Modify.

        let (global_groups_y, output) = Group::create(
            &tx.ctx,
            global_groups_y,
            group_id,
            initial_members.to_owned(),
        )
        .await
        .map_err(ManagerError::Group)?;

        // Write.

        tx.commit(StateChanges::GlobalGroups { global_groups_y })
            .await?;

        self.waiters().notify_events([&output.event]);

        Ok((Group::new(self.clone(), group_id), output))
    }

    /// Get a group by id.
    ///
    /// A group instance provides an API for adding and removing members from a group and querying
    /// the current group members.
    ///
    /// Returns a `PendingGroup` handle which accounts for situations where we want to join a group
    /// but need to still wait for the regarding messages bringing us in. Use `has_group` and
    /// `has_joined_group` to check these conditions without waiting.
    pub fn group(&self, id: impl Into<GroupId>) -> PendingGroup<S, F, C> {
        PendingGroup::new(self.clone(), id.into())
    }

    /// Returns `true` if the group is known.
    pub async fn has_group(&self, id: impl Into<GroupId>) -> Result<bool, ManagerError<F, C>> {
        let global_groups_y = self.read().global_groups_state().await?;
        Ok(global_groups_y.has_group(id.into()))
    }

    /// Returns `true` if we are currently a member of the group.
    pub async fn has_joined_group(
        &self,
        id: impl Into<GroupId>,
    ) -> Result<bool, ManagerError<F, C>> {
        let global_groups_y = self.read().global_groups_state().await?;
        let members = global_groups_y.members(id.into());
        Ok(is_member(&members, self.id()))
    }

    /// Create a new space containing initial members and access levels.
    ///
    /// If not already included, then the local actor (creator of this space) will be added to the
    /// initial members and given manage access level.
    ///
    /// Persists resulting state, returns space instance and forged messages.
    pub async fn create_space(
        &self,
        id: impl Into<SpaceId>,
        initial_members: &[(ActorId, Access<C>)],
    ) -> Result<(Space<S, F, C>, SpaceOutput<C, F::Message>), ManagerError<F, C>> {
        let id = id.into();

        let tx = self.begin_write().await?;

        // Read.

        let global_groups_y = tx.global_groups_state().await?;
        let key_manager_y = tx.key_manager_state().await?;
        let key_registry_y = tx.key_registry_state().await?;

        // Modify.

        let (global_groups_y, space_y, output) = Space::create(
            &tx.ctx,
            global_groups_y,
            key_manager_y,
            key_registry_y,
            id,
            initial_members.to_owned(),
        )
        .await
        .map_err(ManagerError::Space)?;

        // Write.

        tx.commit(StateChanges::Space {
            global_groups_y: Some(global_groups_y),
            space_y,
        })
        .await?;

        self.waiters().notify_events(output.events());

        Ok((Space::new(self.clone(), id), output))
    }

    /// Get a space by id.
    ///
    /// A space instance provides an API for adding and removing members from the space and querying
    /// the current space members.
    ///
    /// Returns a `PendingSpace` handle which accounts for situations where we want to join a space
    /// but need to still wait for the regarding messages bringing us in. Use `has_space` and
    /// `has_joined_space` to check these conditions without waiting.
    pub fn space(&self, id: impl Into<SpaceId>) -> PendingSpace<S, F, C> {
        PendingSpace::new(self.clone(), id.into())
    }

    /// Returns all members who were removed from, and not re-admitted to, this space.
    ///
    /// Returns an empty list if the space is not known yet.
    pub async fn removed_space_members(
        &self,
        id: impl Into<SpaceId>,
    ) -> Result<Vec<MemberId>, ManagerError<F, C>> {
        let Some(space_y) = self.read().space_state(&id.into()).await? else {
            return Ok(vec![]);
        };

        Ok(removed_space_members(&space_y))
    }

    /// Returns `true` if we are currently a member of the space with more than "pull" access.
    pub async fn has_joined_space(
        &self,
        id: impl Into<SpaceId>,
    ) -> Result<bool, ManagerError<F, C>> {
        let id = id.into();

        let Some(space_y) = self.read().space_state(&id).await? else {
            return Ok(false);
        };

        let members = space_y.groups_y.members(space_y.group_id);
        Ok(is_secret_member(&members, self.id()))
    }

    /// Returns `true` if the space is known.
    pub async fn has_space(&self, id: impl Into<SpaceId>) -> Result<bool, ManagerError<F, C>> {
        let id = id.into();

        let has_space = self
            .inner
            .store
            .has_space(&id)
            .await
            .map_err(|err| StoreError::SpacesStore(err.to_string()))?;

        Ok(has_space)
    }

    /// Process a spaces message.
    ///
    /// We expect messages to be signature-checked, dependency-checked & partially ordered.
    ///
    /// Persists resulting state and returns events which inform users of any state changes which
    /// occurred.
    pub async fn process<M>(&self, message: &M) -> Result<Vec<Event<C>>, ManagerError<F, C>>
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

        let tx = self.begin_write().await?;

        // Route message to the regarding member-, group- or space processor. Read and modify.
        let (changes, events) = match args {
            // Received key bundle from a member.
            SpacesArgs::Member(member) => {
                let key_registry_y = tx.key_registry_state().await?;

                let (key_registry_y, event) = self
                    .identity()
                    .process(key_registry_y, member)
                    .map_err(ManagerError::IdentityManager)?;

                (
                    StateChanges::PrekeyBundles {
                        key_manager_y: None,
                        key_registry_y: Some(key_registry_y),
                    },
                    vec![event],
                )
            }

            SpacesArgs::Group { .. } => {
                let global_groups_y = tx.global_groups_state().await?;

                let (global_groups_y, event) =
                    Group::<S, F, C>::process(global_groups_y, &SpacesMessage::auth(message))
                        .map_err(ManagerError::Group)?;

                let changes = match global_groups_y {
                    Some(global_groups_y) => StateChanges::GlobalGroups { global_groups_y },
                    None => StateChanges::Nothing,
                };

                (changes, vec![event])
            }

            // Received control message related to a space.
            SpacesArgs::SpaceMembership { space_id, .. } => {
                let (space_y, events) = self
                    .handle_space_membership_message(
                        &tx,
                        *space_id,
                        &SpacesMessage::space_membership(message),
                    )
                    .await?;

                let changes = match space_y {
                    Some(space_y) => StateChanges::Space {
                        global_groups_y: None,
                        space_y,
                    },
                    None => StateChanges::Nothing,
                };

                (changes, events)
            }

            SpacesArgs::SpaceUpdate { .. } => unimplemented!(),

            // Received encrypted application data for a space.
            SpacesArgs::Application { space_id, .. } => {
                let Some(space_y) = tx.space_state(space_id).await? else {
                    return Err(ManagerError::UnexpectedMessage(message.hash()));
                };

                let (space_y, events) = Space::<S, F, C>::handle_application_message(
                    space_y,
                    &SpacesMessage::application(message),
                )
                .map_err(ManagerError::Space)?;

                (
                    StateChanges::Space {
                        global_groups_y: None,
                        space_y,
                    },
                    events,
                )
            }
        };

        // Write.

        tx.commit(changes).await?;

        self.waiters().notify_events(&events);

        Ok(events)
    }

    /// The public key of the local actor.
    pub fn id(&self) -> ActorId {
        self.actor_id
    }

    /// The local actor id and their long-term key bundle.
    ///
    /// Note: Key bundle will be rotated if the latest is reaching it's configured expiry date.
    pub async fn me(&self) -> Result<AuthenticatedMember, ManagerError<F, C>> {
        let identity = self.identity();

        // Most of the time no rotation is required, we can avoid acquiring a transaction then.
        let reader = self.read();
        let key_manager_y = reader.key_manager_state().await?;
        if !identity.key_bundle_expired(&key_manager_y)? {
            let key_registry_y = reader.key_registry_state().await?;
            let (_, _, member) = identity.me(
                key_manager_y,
                key_registry_y,
                &self.inner.credentials,
                &self.inner.rng,
            )?;
            return Ok(member);
        }

        let tx = self.begin_write().await?;

        // Read.

        let key_manager_y = tx.key_manager_state().await?;
        let key_registry_y = tx.key_registry_state().await?;

        // Modify.

        let (key_manager_y, key_registry_y, member) = identity.me(
            key_manager_y,
            key_registry_y,
            tx.ctx.credentials,
            tx.ctx.rng,
        )?;

        // Write.

        tx.commit(StateChanges::PrekeyBundles {
            key_manager_y: Some(key_manager_y),
            key_registry_y: Some(key_registry_y),
        })
        .await?;

        // Our own (rotated) key bundle was registered.
        self.waiters().members.notify(&member);

        Ok(member)
    }

    /// Get a member by id.
    ///
    /// Returns a `PendingMember` handle which can be awaited until a valid key bundle of the member
    /// was registered. Use `member_key_bundle` to check this without waiting.
    pub fn member(&self, id: impl Into<MemberId>) -> PendingMember<S, F, C> {
        PendingMember::new(self.clone(), id.into())
    }

    /// Returns the latest valid key bundle of a member.
    ///
    /// Returns `None` if no key bundle of the member is known or all of them expired.
    pub async fn member_key_bundle(
        &self,
        id: impl Into<MemberId>,
    ) -> Result<Option<LongTermKeyBundle>, ManagerError<F, C>> {
        let key_registry_y = self.read().key_registry_state().await?;

        match <KeyRegistry<MemberId> as PreKeyRegistry<MemberId, LongTermKeyBundle>>::key_bundle(
            key_registry_y,
            &id.into(),
        ) {
            Ok((_, key_bundle)) => Ok(key_bundle),
            Err(KeyRegistryError::KeyBundlesExpired) => Ok(None),
            Err(err) => Err(ManagerError::IdentityManager(err.into())),
        }
    }

    /// Register a member with long-term key bundle material which was provided through another
    /// channel (QR code scan etc.).
    pub async fn register_member(
        &self,
        member: &AuthenticatedMember,
    ) -> Result<(), ManagerError<F, C>> {
        let tx = self.begin_write().await?;

        // Read.

        let key_registry_y = tx.key_registry_state().await?;

        // Modify.

        let (key_registry_y, event) = self
            .identity()
            .process(key_registry_y, member)
            .map_err(ManagerError::IdentityManager)?;

        // Write.

        tx.commit(StateChanges::PrekeyBundles {
            key_manager_y: None,
            key_registry_y: Some(key_registry_y),
        })
        .await?;

        self.waiters().notify_events([&event]);

        Ok(())
    }

    /// Check if my latest key bundle has expired.
    ///
    /// If `true` then users should rotate their pre-key and generate a new bundle message (which
    /// should then be published) by calling `key_bundle_message`.
    pub async fn key_bundle_expired(&self) -> Result<bool, ManagerError<F, C>> {
        let key_manager_y = self.read().key_manager_state().await?;
        Ok(self.identity().key_bundle_expired(&key_manager_y)?)
    }

    /// Forge a key bundle message containing my latest key bundle.
    ///
    /// Note: Key bundle will be rotated if the latest is reaching it's configured expiry date.
    pub async fn key_bundle_message(&self) -> Result<F::Message, ManagerError<F, C>> {
        let identity = self.identity();

        let tx = self.begin_write().await?;

        // Read.

        let key_manager_y = tx.key_manager_state().await?;
        let key_registry_y = tx.key_registry_state().await?;

        // Keys only change when the key bundle needs to be rotated.
        let rotated = identity.key_bundle_expired(&key_manager_y)?;

        // Modify.

        let (key_manager_y, key_registry_y, message) = identity
            .key_bundle_message(
                key_manager_y,
                key_registry_y,
                tx.ctx.credentials,
                tx.ctx.rng,
                tx.ctx.forge,
            )
            .await?;

        // Write.

        let changes = if rotated {
            StateChanges::PrekeyBundles {
                key_manager_y: Some(key_manager_y),
                key_registry_y: Some(key_registry_y),
            }
        } else {
            StateChanges::default()
        };

        tx.commit(changes).await?;

        // Our own key bundle was registered if it got rotated.
        if rotated && let SpacesArgs::Member(member) = message.borrow() {
            self.waiters().members.notify(member);
        }

        Ok(message)
    }

    /// Return `true` if the space is missing operations from the passed groups.
    pub async fn space_repair_required(
        &self,
        space_id: SpaceId,
        groups: &[GroupId],
    ) -> Result<bool, ManagerError<F, C>> {
        let reader = self.read();

        let Some(space_y) = reader.space_state(&space_id).await? else {
            return Err(ManagerError::SpaceNotFound(space_id));
        };
        let global_groups_y = reader.global_groups_state().await?;

        let space_heads = space_y.groups_y.inner.heads(groups);
        let global_heads = global_groups_y.inner.heads(groups);

        Ok(space_heads != global_heads)
    }

    /// Publish a reference to any auth messages missing from a space.
    ///
    /// Each space holds a copy of the shared auth state by publishing a reference to each auth
    /// control message it witnesses. A space can get out-of-sync with this shared state if auth
    /// messages were published without the local peer knowing about a space, either because they
    /// are not a member or because they were yet to learn about it.
    ///
    /// ## Out-of-sync Space
    ///
    /// ```text
    /// Shared Auth State     Space State
    ///
    ///       [x]
    ///       [x] <-------------- [z]
    ///       [x] <-------------- [z]
    ///       [x] <-------------- [z]
    /// ```
    ///
    /// On identifying that a space needs "repairing" by calling space_repair_required(), _any_
    /// current space member can publish a message into the space referencing the missing auth
    /// message.
    ///
    /// It is recommended that repair does not occur after every call to process() as this would
    /// cause peers to publish redundant pointers into the spaces graph. Although these duplicates do not
    /// introduce any buggy or unexpected behavior, repairing after every processed message would
    /// introduce an undesirable level of redundancy.
    ///
    /// ## Redundant pointers
    ///
    /// ```text
    /// Shared Auth State     Space State
    ///
    ///       [x] <-----------[z1][z2][z3]
    ///       [x] <-------------- [z]
    ///       [x] <-------------- [z]
    ///       [x] <-------------- [z]
    /// ```
    ///
    /// A sensible approach to detecting and repairing spaces will involve processing messages in
    /// logical batches and only detecting and repairing any out-of-sync spaces after a batch has
    /// been processed. Alternatively some scheduling or throttling logic could be employed.
    ///
    /// Persists resulting state and returns forged messages.
    pub async fn repair_space(
        &self,
        space_id: SpaceId,
        groups: &[GroupId],
    ) -> Result<RepairOutput<C, F::Message>, ManagerError<F, C>> {
        let tx = self.begin_write().await?;

        // Read.

        let Some(space_y) = tx.space_state(&space_id).await? else {
            return Err(ManagerError::SpaceNotFound(space_id));
        };

        // Only members with Read or greater access can repair spaces.
        let can_repair = space_y
            .groups_y
            .members(space_y.group_id)
            .iter()
            .any(|(id, access)| *id == self.id() && *access >= Access::<C>::read());

        if !can_repair {
            tx.rollback().await?;
            return Ok(RepairOutput::from(vec![]));
        }

        let global_groups_y = tx.global_groups_state().await?;

        // Modify.

        let (space_y, output) = Space::apply_repair(&tx.ctx, &global_groups_y, space_y, groups)
            .await
            .map_err(ManagerError::Space)?;

        // Write.

        if let Some(space_y) = space_y {
            tx.commit(StateChanges::Space {
                global_groups_y: None,
                space_y,
            })
            .await?;
        } else {
            tx.rollback().await?;
        }

        self.waiters().notify_events(output.events());

        Ok(output)
    }

    /// Returns resulting space state (`None` if nothing changed) and events.
    #[allow(clippy::type_complexity)]
    async fn handle_space_membership_message(
        &self,
        tx: &StateTransaction<'_, S, F, C>,
        space_id: SpaceId,
        message: &SpaceMembershipMessage,
    ) -> Result<(Option<SpaceState<C>>, Vec<Event<C>>), ManagerError<F, C>> {
        // Get auth message.
        let auth_message = {
            let auth_message_id = message.auth_message_id;
            let Some(message) = tx
                .ctx
                .store
                .get_spaces_message_tx(&auth_message_id)
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

        let space_y = match tx.space_state(&space_id).await? {
            Some(space_y) => space_y,
            None => {
                if !auth_message.action().is_create() {
                    // If this is not a "create" message we should have learned about the space
                    // before. This can be either a faulty message or a problem with the message
                    // orderer.
                    return Err(ManagerError::UnexpectedMessage(message.id));
                }

                // Initialise a new space from the "create" message.
                SpaceState::init(
                    self.id(),
                    space_id,
                    message.group_id,
                    tx.key_manager_state().await?,
                    tx.key_registry_state().await?,
                )
            }
        };

        Space::handle_membership_message(&tx.ctx, space_y, message, &auth_message)
            .map_err(ManagerError::Space)
    }
}

#[cfg(any(test, feature = "test_utils"))]
impl<S, F, C> Manager<S, F, C>
where
    S: Clone
        + SpacesStore<SpacesStoreState<C>>
        + SpacesMessageStore<F::Message>
        + GroupsStore<AuthMessage<C>, C>
        + KeyRegistryStore
        + PreKeyBundlesStore
        + Transaction,
    F: Forge<C>,
    C: Conditions,
{
    /// Get the latest committed global auth state.
    pub async fn get_global_groups_state(
        &self,
    ) -> Result<crate::types::AuthGroupState<C>, ManagerError<F, C>> {
        Ok(self.read().global_groups_state().await?)
    }

    /// Get the latest committed space state.
    pub async fn get_space_state(
        &self,
        id: &SpaceId,
    ) -> Result<Option<SpaceState<C>>, ManagerError<F, C>> {
        Ok(self.read().space_state(id).await?)
    }

    /// Repair all given spaces, each in their own transaction.
    pub async fn repair_spaces(
        &self,
        space_ids: &[SpaceId],
    ) -> Result<Vec<RepairOutput<C, F::Message>>, ManagerError<F, C>> {
        let mut results = vec![];

        for id in space_ids {
            let Some(space_y) = self.get_space_state(id).await? else {
                continue;
            };

            let result = self.repair_space(*id, &[space_y.group_id]).await?;
            results.push(result);
        }

        Ok(results)
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

    #[error(transparent)]
    State(#[from] StateError),
}
