// SPDX-License-Identifier: MIT OR Apache-2.0

//! API for managing members of a space and sending/receiving messages.
use std::collections::HashSet;
use std::convert::Infallible;
use std::fmt::Debug;

use p2panda_auth::Access;
use p2panda_auth::traits::{Conditions, Operation};
use p2panda_encryption::traits::GroupMessage;
use p2panda_encryption::{Rng, RngError};
use petgraph::algo::toposort;
use thiserror::Error;

use crate::auth::message::AuthMessage;
use crate::auth::orderer::AuthOrdererState;
use crate::encryption::dgm::EncryptionMembershipState;
use crate::encryption::message::{EncryptionArgs, EncryptionMessage};
use crate::encryption::orderer::EncryptionOrdererState;
use crate::event::{Event, encryption_output_to_space_events, space_message_to_space_event};
use crate::group::{Group, GroupError};
use crate::identity::IdentityError;
use crate::manager::Manager;
use crate::message::SpacesArgs;
use crate::traits::{
    AuthStore, AuthoredMessage, Forge, KeyRegistryStore, KeySecretStore, MessageStore, SpaceId,
    SpacesMessage, SpacesStore,
};
use crate::types::{
    ActorId, AuthGroup, AuthGroupAction, AuthGroupError, AuthGroupState, AuthResolver,
    EncryptionDirectMessage, EncryptionGroup, EncryptionGroupError, EncryptionGroupState,
    OperationId,
};
use crate::utils::{added_members, removed_members, secret_members, sort_members};

/// A single encryption context with associated group of actors who will participate in the key
/// agreement protocol.
///
/// Members in the encryption context can publish application messages to the group which will be
/// encrypted with the latest group secret. All other members will be able to decrypt and read the
/// message.
///
/// Actors can be added or removed from the space; an actor may be an individual or a group.
/// Access levels are assigned to all members, these access levels can be used by authorisation
/// layers outside of p2panda-spaces to enforce access control rules.
///
/// Members with only Pull access are not included in the encryption context and won't receive any
/// group secrets. Only members with Manage access level are allowed to manage the groups members.
#[derive(Debug)]
pub struct Space<ID, S, K, F, M, C, RS> {
    /// Reference to the manager.
    ///
    /// This allows us build an API where users can treat "space" instances independently from the
    /// manager API, even though internally it has a reference to it.
    manager: Manager<ID, S, K, F, M, C, RS>,

    /// Id of the space.
    ///
    /// This is the "pointer" at the related space state which lives inside the manager.
    id: ID,
}

impl<ID, S, K, F, M, C, RS> Space<ID, S, K, F, M, C, RS>
where
    ID: SpaceId,
    S: SpacesStore<ID, M, C> + AuthStore<C> + MessageStore<M> + Debug,
    K: KeyRegistryStore + KeySecretStore + Debug,
    F: Forge<ID, M, C> + Debug,
    M: AuthoredMessage + SpacesMessage<ID, C> + Debug,
    C: Conditions,
    RS: Debug + AuthResolver<C>,
{
    pub(crate) fn new(manager_ref: Manager<ID, S, K, F, M, C, RS>, id: ID) -> Self {
        Self {
            manager: manager_ref,
            id,
        }
    }

    /// Create a space containing initial members and access levels.
    ///
    /// If not already included, then the local actor (creator of this space) will be added to the
    /// initial members and given manage access level.
    ///
    /// Returns messages for replication to other instances and events which inform users of any
    /// state changes which occurred.
    pub(crate) async fn create(
        manager_ref: Manager<ID, S, K, F, M, C, RS>,
        space_id: ID,
        mut initial_members: Vec<(ActorId, Access<C>)>,
    ) -> Result<(Self, Vec<M>, Vec<Event<ID, C>>), SpaceError<ID, S, K, F, M, C, RS>> {
        let my_id = manager_ref.id();

        // Get the global auth state. We use this state in a following step to initialise the
        // space state and we don't want it to contain the group for the space itself.
        let auth_y = {
            let manager = manager_ref.inner.read().await;
            manager.store.auth().await.map_err(SpaceError::AuthStore)?
        };

        // Automatically add ourselves with "manage" level without any conditions as default.
        if !initial_members.iter().any(|(member, _)| *member == my_id) {
            initial_members.push((my_id, Access::manage()));
        }

        // Create new group for the space.
        let (group, mut messages, auth_event) = Group::create(manager_ref.clone(), initial_members)
            .await
            .map_err(SpaceError::Group)?;

        // Instantiate new space state from existing global auth state.
        let y = Self::state_from_auth(
            manager_ref.clone(),
            auth_y,
            space_id,
            group.id(),
            &mut messages,
        )
        .await?;

        // Apply the "create" auth message to the space state.
        //
        // We know the first message is the auth message.
        let (space_message, space_event) =
            Self::process_auth_message(manager_ref.clone(), y, &messages[0]).await?;

        messages.push(space_message.expect("creating space results in message"));
        let space_event = space_event.expect("creating space results in event");

        Ok((
            Self {
                id: space_id,
                manager: manager_ref,
            },
            messages,
            vec![auth_event, space_event],
        ))
    }

    /// Add a member to the space with assigned access level.
    ///
    /// Returns messages for replication to other instances and events which inform users of any
    /// state changes which occurred.
    pub async fn add(
        &self,
        member: ActorId,
        access: Access<C>,
    ) -> Result<(Vec<M>, Vec<Event<ID, C>>), SpaceError<ID, S, K, F, M, C, RS>> {
        let y = self.state().await?;

        // If the space exists we can assume the associated group exists.
        let group = Group::new(self.manager.clone(), y.group_id);
        group.add(member, access).await.map_err(SpaceError::Group)
    }

    /// Remove a member from the space.
    ///
    /// Returns messages for replication to other instances and events which inform users of any
    /// state changes which occurred.
    pub async fn remove(
        &self,
        member: ActorId,
    ) -> Result<(Vec<M>, Vec<Event<ID, C>>), SpaceError<ID, S, K, F, M, C, RS>> {
        let y = self.state().await?;
        // If the space exists we can assume the associated group exists.
        let group = Group::new(self.manager.clone(), y.group_id);
        group.remove(member).await.map_err(SpaceError::Group)
    }

    /// Forge a "pointer" space message from an already existing auth message and apply any
    /// resulting group membership changes. Any resulting encryption direct messages are included
    /// in the space message alongside a reference to the auth message.
    pub(crate) async fn process_auth_message(
        manager_ref: Manager<ID, S, K, F, M, C, RS>,
        mut y: SpaceState<ID, M, C>,
        auth_message: &M,
    ) -> Result<(Option<M>, Option<Event<ID, C>>), SpaceError<ID, S, K, F, M, C, RS>> {
        if y.auth_y.inner.operations.contains_key(&auth_message.id()) {
            return Ok((None, None));
        }

        // Get current space members.
        let current_members = secret_members(y.auth_y.members(y.group_id));

        // Process auth message on local auth state.
        let auth_message = AuthMessage::from_forged(auth_message);
        y.auth_y = AuthGroup::process(y.auth_y, &auth_message).map_err(SpaceError::AuthGroup)?;

        // Get next space members.
        let next_members = secret_members(y.auth_y.members(y.group_id));

        // Process the change of membership on encryption the context.
        let (encryption_y, direct_messages) = if current_members != next_members {
            let manager = manager_ref.inner.read().await;
            Self::apply_secret_member_change(
                y.encryption_y,
                &auth_message,
                current_members.clone(),
                next_members.clone(),
                &manager.rng,
            )
            .await?
        } else {
            (y.encryption_y, vec![])
        };
        y.encryption_y = encryption_y;

        // Construct space message and sign it in the forge (K)
        let dependencies: Vec<OperationId> = y.encryption_y.orderer.heads().to_vec();
        let space_message = {
            let args = SpacesArgs::SpaceMembership {
                space_id: y.space_id,
                group_id: y.group_id,
                space_dependencies: dependencies.clone(),
                auth_message_id: auth_message.id(),
                direct_messages,
            };

            let mut manager = manager_ref.inner.write().await;
            manager.identity.forge(args).await?
        };

        // Update space state and persist it.
        {
            let manager = manager_ref.inner.write().await;
            y.encryption_y
                .orderer
                .add_dependency(space_message.id(), &dependencies);

            let space_id = y.space_id;
            manager
                .store
                .set_space(&space_id, y)
                .await
                .map_err(SpaceError::SpacesStore)?;
        }

        // If current and next member sets are equal it indicates that the space is not affected
        // by this auth change. This can be because the space wasn't created yet, or the auth
        // change simply does not effect the members of this space. In either case we don't want
        // to emit any membership change event.
        if current_members == next_members {
            return Ok((Some(space_message), None));
        };

        // Construct space membership event.
        let space_event = space_message_to_space_event(
            &space_message,
            &auth_message,
            current_members,
            next_members,
        );

        Ok((Some(space_message), Some(space_event)))
    }

    /// Process a space message along with it's relevant auth message (if required).
    ///
    /// Returns events which inform users of any state changes which occurred.
    pub(crate) async fn process(
        &self,
        space_message: &M,
        auth_message: Option<&AuthMessage<C>>,
    ) -> Result<Vec<Event<ID, C>>, SpaceError<ID, S, K, F, M, C, RS>> {
        let events = match space_message.args() {
            SpacesArgs::SpaceMembership { space_id, .. } => {
                assert_eq!(space_id, &self.id); // Sanity check.
                let auth_message =
                    auth_message.expect("all space membership messages have auth message");
                self.handle_membership_message(space_message, auth_message)
                    .await?
            }
            SpacesArgs::Application { space_id, .. } => {
                assert_eq!(space_id, &self.id); // Sanity check.
                self.handle_application_message(space_message).await?
            }
            _ => panic!("unexpected message"),
        };

        Ok(events)
    }

    /// Instantiate space state from existing global auth state.
    ///
    /// Every space contains pointers to all messages published to the global auth state. This
    /// method iterates through all existing auth messages and publishes these pointers to the
    /// space. None of the messages will contain encryption control messages as they were
    /// published before the space existed.
    async fn state_from_auth(
        manager_ref: Manager<ID, S, K, F, M, C, RS>,
        auth_y: AuthGroupState<C>,
        space_id: ID,
        group_id: ActorId,
        messages: &mut Vec<M>,
    ) -> Result<SpaceState<ID, M, C>, SpaceError<ID, S, K, F, M, C, RS>> {
        // Instantiate empty space state.
        let mut y = { Self::get_or_init_state(space_id, group_id, manager_ref.clone()).await? };

        // Publish pointers for all operations in the global auth graph. We topologically sort the
        // operations and publish them in this linear order.
        //
        // These won't contain any encryption messages as they were published _before_ the space
        // was created.
        let mut manager = manager_ref.inner.write().await;
        let mut space_dependencies = vec![];
        let operations =
            toposort(&auth_y.inner.graph, None).expect("auth graph does not contain cycles");
        for id in operations {
            let operation = auth_y
                .inner
                .operations
                .get(&id)
                .expect("all auth operations exist");

            let args = SpacesArgs::SpaceMembership {
                space_id: y.space_id,
                group_id: y.group_id,
                auth_message_id: operation.id(),
                direct_messages: vec![],
                space_dependencies,
            };
            let message = manager.identity.forge(args).await?;

            space_dependencies = vec![message.id()];
            messages.push(message);
        }
        y.auth_y = auth_y;
        Ok(y)
    }

    /// Handle messages which effect the space membership. Each of these messages contained a
    /// pointer to an auth message and the auth message is required here.
    async fn handle_membership_message(
        &self,
        space_message: &M,
        auth_message: &AuthMessage<C>,
    ) -> Result<Vec<Event<ID, C>>, SpaceError<ID, S, K, F, M, C, RS>> {
        let SpacesArgs::SpaceMembership {
            space_id,
            group_id,
            space_dependencies,
            ..
        } = space_message.args()
        else {
            panic!("unexpected message type");
        };

        // Get space state and current members.
        let mut y = Self::get_or_init_state(self.id, *group_id, self.manager.clone()).await?;

        // If we already processed this message return here.
        if y.encryption_y.orderer.has_seen(space_message.id()) {
            return Ok(vec![]);
        }

        let duplicate_pointer = y.auth_y.inner.operations.contains_key(&auth_message.id());

        let mut current_members = secret_members(y.auth_y.members(y.group_id));
        current_members.sort();

        // Process auth message on space auth state.
        //
        // Skip processing if this auth message has already been processed. This can happen when
        // multiple peers concurrently publish pointers to some auth message into the space.
        let next_members = if !duplicate_pointer {
            y.auth_y = AuthGroup::process(y.auth_y, auth_message).map_err(SpaceError::AuthGroup)?;
            // Get next space members.
            let mut next_members = secret_members(y.auth_y.members(y.group_id));
            next_members.sort();
            next_members
        } else {
            current_members.clone()
        };

        // Make the dgm aware of the new space members.
        y.encryption_y.dcgka.dgm.members = HashSet::from_iter(next_members.clone());

        // Construct encryption message.
        //
        // We do this even when the auth message was not processed above, to make sure that we
        // still consume all direct messages.
        let my_id = self.manager.id();
        let encryption_message = EncryptionMessage::from_membership(
            space_message,
            my_id,
            auth_message,
            &current_members,
            &next_members,
        );

        // Process encryption message.
        let (encryption_y, encryption_output) =
            EncryptionGroup::receive(y.encryption_y, &encryption_message)
                .map_err(SpaceError::EncryptionGroup)?;

        y.encryption_y = encryption_y;

        // Persist new space state.

        {
            let manager = self.manager.inner.write().await;
            y.encryption_y
                .orderer
                .add_dependency(space_message.id(), space_dependencies);
            manager
                .store
                .set_space(&self.id, y)
                .await
                .map_err(SpaceError::SpacesStore)?;
        }

        let events = if !duplicate_pointer {
            let mut events = encryption_output_to_space_events(space_id, encryption_output);

            // If current and next member sets are equal it indicates that the space is not affected
            // by this auth change. This can be because the space wasn't created yet, or the auth
            // change simply does not effect the members of this space. In either case we don't want
            // to emit any membership change event.
            if current_members == next_members {
                return Ok(events);
            };

            // Construct space membership event.
            let membership_event = space_message_to_space_event(
                space_message,
                auth_message,
                current_members,
                next_members,
            );

            // Insert membership event at front of vec.
            events.insert(0, membership_event);
            events
        } else {
            vec![]
        };

        Ok(events)
    }

    /// Apply a group membership change to the group encryption state.
    ///
    /// For "add" and "remove" actions, the difference between the current and next secret group
    /// members (those with "read" access) is computed and only the diff processed on the
    /// encryption group.
    async fn apply_secret_member_change(
        mut encryption_y: EncryptionGroupState<M>,
        auth_message: &AuthMessage<C>,
        current_members: Vec<ActorId>,
        next_members: Vec<ActorId>,
        rng: &Rng,
    ) -> Result<
        (EncryptionGroupState<M>, Vec<EncryptionDirectMessage>),
        SpaceError<ID, S, K, F, M, C, RS>,
    > {
        // Make the DGM aware of group members after this group membership change has been
        // processed.
        encryption_y.dcgka.dgm = EncryptionMembershipState {
            members: HashSet::from_iter(next_members.clone().into_iter()),
        };

        let mut direct_messages = vec![];
        let encryption_y = {
            match &auth_message.payload().action {
                AuthGroupAction::Create { .. } => {
                    let (encryption_y, message) =
                        EncryptionGroup::create(encryption_y, next_members.clone(), rng)
                            .map_err(SpaceError::EncryptionGroup)?;
                    direct_messages.extend(message.direct_messages());
                    encryption_y
                }
                AuthGroupAction::Add { .. } => {
                    let all_added = added_members(current_members, next_members);

                    if all_added.is_empty() {
                        return Ok((encryption_y, vec![]));
                    }

                    for added in all_added {
                        let (encryption_y_inner, message) =
                            EncryptionGroup::add(encryption_y, added, rng)
                                .map_err(SpaceError::EncryptionGroup)?;
                        encryption_y = encryption_y_inner;
                        direct_messages.extend(message.direct_messages());
                    }
                    encryption_y
                }
                AuthGroupAction::Remove { .. } => {
                    let all_removed = removed_members(current_members, next_members);

                    if all_removed.is_empty() {
                        return Ok((encryption_y, vec![]));
                    }

                    for removed in all_removed {
                        let (encryption_y_inner, message) =
                            EncryptionGroup::remove(encryption_y, removed, rng)
                                .map_err(SpaceError::EncryptionGroup)?;
                        encryption_y = encryption_y_inner;
                        direct_messages.extend(message.direct_messages());
                    }
                    encryption_y
                }
                _ => unimplemented!(),
            }
        };

        Ok((encryption_y, direct_messages))
    }

    /// Handle space application messages.
    async fn handle_application_message(
        &self,
        message: &M,
    ) -> Result<Vec<Event<ID, C>>, SpaceError<ID, S, K, F, M, C, RS>> {
        let SpacesArgs::Application {
            space_dependencies, ..
        } = message.args()
        else {
            panic!("unexpected message type")
        };

        let mut y = self.state().await?;

        // Process encryption message.
        let encryption_message = EncryptionMessage::from_application(message);
        let (encryption_y, encryption_output) = {
            EncryptionGroup::receive(y.encryption_y, &encryption_message)
                .map_err(SpaceError::EncryptionGroup)?
        };

        y.encryption_y = encryption_y;

        // Update dependencies.
        y.encryption_y
            .orderer
            .add_dependency(encryption_message.id(), space_dependencies);

        // Persist new state.
        let events = encryption_output_to_space_events(&y.space_id, encryption_output);
        let manager = self.manager.inner.write().await;
        manager
            .store
            .set_space(&self.id, y)
            .await
            .map_err(SpaceError::SpacesStore)?;

        Ok(events)
    }

    /// Sync a shared auth state change with this space.
    pub(crate) async fn handle_auth_group_change(
        &self,
        auth_message: &M,
    ) -> Result<(Option<M>, Option<Event<ID, C>>), SpaceError<ID, S, K, F, M, C, RS>> {
        // If this space already processed this auth message then skip it.
        let y = self.state().await?;
        if y.auth_y.inner.operations.contains_key(&auth_message.id()) {
            return Ok((None, None));
        }

        let my_id = self.manager.id();
        let is_reader = self
            .members()
            .await?
            .iter()
            .any(|(member, access)| *member == my_id && access > &Access::pull());

        if is_reader {
            return Space::process_auth_message(self.manager.clone(), y, auth_message).await;
        }

        Ok((None, None))
    }

    /// Get the space state.
    pub(crate) async fn state(
        &self,
    ) -> Result<SpaceState<ID, M, C>, SpaceError<ID, S, K, F, M, C, RS>> {
        let manager = self.manager.inner.read().await;
        let mut space_y = manager
            .store
            .space(&self.id)
            .await
            .map_err(SpaceError::SpacesStore)?
            .ok_or(SpaceError::UnknownSpace(self.id))?;

        // Inject latest key material to space DCGKA state.
        let key_manager_y = manager.identity.key_manager().await?;

        let key_registry_y = manager.identity.key_registry().await?;

        space_y.encryption_y.dcgka.my_keys = key_manager_y;
        space_y.encryption_y.dcgka.pki = key_registry_y;

        Ok(space_y)
    }

    /// Get or if not present initialize a new space state.
    async fn get_or_init_state(
        space_id: ID,
        group_id: ActorId,
        manager_ref: Manager<ID, S, K, F, M, C, RS>,
    ) -> Result<SpaceState<ID, M, C>, SpaceError<ID, S, K, F, M, C, RS>> {
        let manager = manager_ref.inner.read().await;

        let key_manager_y = manager.identity.key_manager().await?;

        let key_registry_y = manager.identity.key_registry().await?;

        let result = manager
            .store
            .space(&space_id)
            .await
            .map_err(SpaceError::SpacesStore)?;

        let space_y = match result {
            Some(mut y) => {
                // Inject latest key material to space DCGKA state.
                y.encryption_y.dcgka.my_keys = key_manager_y;
                y.encryption_y.dcgka.pki = key_registry_y;
                y
            }
            None => {
                let my_id = manager.identity.id();

                let dgm = EncryptionMembershipState {
                    members: HashSet::new(),
                };

                // Encryption orderer state is empty when we're initializing a new encryption
                // state.
                let encryption_orderer_y = EncryptionOrdererState::new();

                // Auth orderer inside of a space is never used, AuthGroup::prepare is never
                // called and we expect messages in a space to arrive based on space
                // dependencies being satisfied.
                let auth_orderer_y = AuthOrdererState::new();

                let encryption_y = EncryptionGroup::init(
                    my_id,
                    key_manager_y,
                    key_registry_y,
                    dgm,
                    encryption_orderer_y,
                );

                SpaceState::from_state(
                    space_id,
                    group_id,
                    AuthGroupState::new(auth_orderer_y),
                    encryption_y,
                )
            }
        };

        // @TODO: This is ugly, improve space initialization code so that we don't have to pass in
        // the group id like we do now.
        //
        // Sanity check.
        assert_eq!(space_y.group_id, group_id);
        Ok(space_y)
    }

    /// Id of this space.
    pub fn id(&self) -> ID {
        self.id
    }

    /// Id of the group associated with this space.
    pub async fn group_id(&self) -> Result<ActorId, SpaceError<ID, S, K, F, M, C, RS>> {
        let y = self.state().await?;
        Ok(y.group_id)
    }

    /// The members of this space.
    pub async fn members(
        &self,
    ) -> Result<Vec<(ActorId, Access<C>)>, SpaceError<ID, S, K, F, M, C, RS>> {
        let y = self.state().await?;
        let mut group_members = y.auth_y.members(y.group_id);
        sort_members(&mut group_members);
        Ok(group_members)
    }

    /// Publish a message encrypted towards all current group members.
    pub async fn publish(&self, plaintext: &[u8]) -> Result<M, SpaceError<ID, S, K, F, M, C, RS>> {
        let mut y = self.state().await?;

        if !y.encryption_y.orderer.is_welcomed() {
            return Err(SpaceError::NotWelcomed(self.id()));
        }

        let mut manager = self.manager.inner.write().await;

        // Encrypt plaintext towards encryption group members.
        let (encryption_y, encryption_args) =
            EncryptionGroup::send(y.encryption_y, plaintext, &manager.rng)
                .map_err(SpaceError::EncryptionGroup)?;
        y.encryption_y = encryption_y;

        // Construct space args.
        let (args, dependencies) = {
            let EncryptionMessage::Args(encryption_args) = encryption_args else {
                panic!("here we're only dealing with local operations");
            };

            let EncryptionArgs::Application {
                dependencies,
                group_secret_id,
                nonce,
                ciphertext,
            } = encryption_args
            else {
                panic!("unexpected message type");
            };
            let args = SpacesArgs::Application {
                space_id: y.space_id,
                space_dependencies: dependencies.clone(),
                group_secret_id,
                nonce,
                ciphertext,
            };
            (args, dependencies)
        };

        // Forge message.
        let message = manager.identity.forge(args).await?;

        // Update dependencies.
        y.encryption_y
            .orderer
            .add_dependency(message.id(), &dependencies);

        // Persist space state.
        manager
            .store
            .set_space(&self.id, y)
            .await
            .map_err(SpaceError::SpacesStore)?;

        Ok(message)
    }
}

/// Space state object.
#[derive(Debug)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct SpaceState<ID, M, C>
where
    C: Conditions,
{
    pub space_id: ID,
    pub group_id: ActorId,
    pub auth_y: AuthGroupState<C>,
    // @TODO: This contains the PKI and KMG states and other unnecessary data we don't need to
    // persist. We can make the fields public in `p2panda-encryption` and extract only the
    // information we really need.
    pub encryption_y: EncryptionGroupState<M>,
}

impl<ID, M, C> SpaceState<ID, M, C>
where
    ID: SpaceId,
    C: Conditions,
{
    pub fn from_state(
        space_id: ID,
        group_id: ActorId,
        auth_y: AuthGroupState<C>,
        encryption_y: EncryptionGroupState<M>,
    ) -> Self {
        Self {
            space_id,
            group_id,
            auth_y,
            encryption_y,
        }
    }
}

/// Space error type.
#[derive(Debug, Error)]
pub enum SpaceError<ID, S, K, F, M, C, RS>
where
    ID: SpaceId,
    S: SpacesStore<ID, M, C> + AuthStore<C> + MessageStore<M>,
    K: KeyRegistryStore + KeySecretStore + Debug,
    F: Forge<ID, M, C> + Debug,
    C: Conditions,
    RS: AuthResolver<C> + Debug,
{
    #[error(transparent)]
    Rng(#[from] RngError),

    #[error("{0}")]
    AuthGroup(AuthGroupError<C, RS>),

    #[error("{0}")]
    Group(GroupError<ID, S, K, F, M, C, RS>),

    #[error("{0}")]
    EncryptionGroup(EncryptionGroupError<M>),

    #[error(transparent)]
    IdentityManager(#[from] IdentityError<ID, K, F, M, C>),

    #[error("{0}")]
    AuthStore(<S as AuthStore<C>>::Error),

    #[error("{0}")]
    MessageStore(<S as MessageStore<M>>::Error),

    #[error("{0}")]
    SpacesStore(<S as SpacesStore<ID, M, C>>::Error),

    #[error("{0}")]
    EncryptionOrderer(Infallible),

    #[error("tried to access unknown space id {0}")]
    UnknownSpace(ID),

    #[error("tried to publish when not a member of space {0}")]
    NotWelcomed(ID),
}
