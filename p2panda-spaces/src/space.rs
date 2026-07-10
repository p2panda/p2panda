// SPDX-License-Identifier: MIT OR Apache-2.0

//! API for managing members of a space and sending/receiving messages.
use std::fmt::Debug;

use p2panda_auth::Access;
use p2panda_auth::traits::{Conditions, Operation};
use p2panda_core::traits::{Digest, ShortFormat};
use p2panda_core::{SigningKey, VerifyingKey};
use p2panda_encryption::key_manager::KeyManagerState;
use p2panda_encryption::key_registry::KeyRegistryState;
use p2panda_encryption::traits::GroupMessage;
use p2panda_encryption::{Rng, RngError};
use p2panda_store::Transaction;
use p2panda_store::groups::GroupsStore;
use p2panda_store::key_registry::KeyRegistryStore;
use p2panda_store::key_secrets::KeySecretsStore;
use p2panda_store::spaces::{SpacesMessageStore, SpacesStore};
use petgraph::algo::toposort;
use thiserror::Error;
use tracing::debug;

use crate::auth::message::AuthMessage;
use crate::encryption::dgm::EncryptionMembershipState;
use crate::encryption::message::{EncryptionArgs, EncryptionMessage};
use crate::encryption::orderer::EncryptionOrdererState;
use crate::event::{Event, encryption_output_to_space_events, space_message_to_space_event};
use crate::forge::Forge;
use crate::group::{Group, GroupError};
use crate::identity::IdentityError;
use crate::manager::{Manager, StoreError};
use crate::message::{ApplicationMessage, SpaceMembershipMessage, SpacesArgs, SpacesMessage};
use crate::store::SpacesStoreState;
use crate::types::{
    AuthGroup, AuthGroupAction, AuthGroupError, AuthGroupState, AuthResolver,
    EncryptionDirectMessage, EncryptionGroup, EncryptionGroupError, EncryptionGroupState,
};
use crate::utils::{added_members, removed_members, secret_members, sort_members};
use crate::{ActorId, GroupId, MemberId, OperationId, SpaceEvent, SpaceId};

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
pub struct Space<S, F, C, RS> {
    /// Reference to the manager.
    ///
    /// This allows us build an API where users can treat "space" instances independently from the
    /// manager API, even though internally it has a reference to it.
    manager: Manager<S, F, C, RS>,

    /// Id of the space.
    ///
    /// This is the "pointer" at the related space state which lives inside the manager.
    id: SpaceId,
}

impl<S, F, C, RS> Space<S, F, C, RS>
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
    RS: AuthResolver<C>,
{
    pub fn new(manager_ref: Manager<S, F, C, RS>, id: SpaceId) -> Self {
        Self {
            manager: manager_ref,
            id,
        }
    }

    /// Verifying key of the local actor.
    pub fn me(&self) -> VerifyingKey {
        self.manager.id()
    }

    /// Create a space containing initial members and access levels.
    ///
    /// If not already included, then the local actor (creator of this space) will be added to the
    /// initial members and given manage access level.
    ///
    /// Returns resulting auth and space state and messages for processing.
    pub(crate) async fn create(
        manager_ref: Manager<S, F, C, RS>,
        space_id: SpaceId,
        mut initial_members: Vec<(ActorId, Access<C>)>,
    ) -> Result<
        (
            AuthGroupState<C>,
            SpacesState<C>,
            Vec<F::Message>,
            Vec<Event<C>>,
        ),
        SpaceError<F, C>,
    > {
        let my_id = manager_ref.id();

        // Automatically add ourselves with "manage" level without any conditions as default.
        if !initial_members.iter().any(|(member, _)| *member == my_id) {
            initial_members.push((my_id, Access::manage()));
        }

        // Generate random group id.
        let group_id: VerifyingKey = {
            let manager = manager_ref.inner.read().await;
            let signing_key = SigningKey::from_bytes(&manager.rng.random_array()?);
            signing_key.verifying_key()
        };

        // Instantiate new space state from existing global auth state.
        let (space_y, space_history) =
            Self::from_group(manager_ref.clone(), space_id, group_id).await?;

        // Create the space group.
        let groups_y = manager_ref.get_groups_state().await?;

        let (groups_y, create_group, auth_event) =
            Group::create(manager_ref.clone(), groups_y, group_id, initial_members).await?;

        // Apply the "create" auth message to the space state.
        let (space_y, create_space, space_events) = Space::process_auth_message(
            manager_ref.clone(),
            space_y,
            &SpacesMessage::auth(&create_group),
        )
        .await?;

        let mut messages = vec![create_group];
        messages.extend(space_history);
        messages.extend([create_space]);

        let mut events = vec![auth_event];
        events.extend(space_events);

        Ok((groups_y, space_y, messages, events))
    }

    /// Add a member to the space with assigned access level.
    ///
    /// Returns resulting auth and space state and messages for processing.
    pub async fn add(
        &self,
        member: impl Into<ActorId>,
        access: Access<C>,
    ) -> Result<
        (
            AuthGroupState<C>,
            SpacesState<C>,
            F::Message,
            F::Message,
            Vec<Event<C>>,
        ),
        SpaceError<F, C>,
    > {
        let member = member.into();

        let space_y = self.state().await?;
        let group = Group::new(self.manager.clone(), space_y.group_id);

        let (groups_y, auth_message, auth_event) = group.add(member, access).await?;

        let (space_y, space_message, space_events) = Space::process_auth_message(
            self.manager.clone(),
            space_y,
            &SpacesMessage::auth(&auth_message),
        )
        .await?;

        let mut events = vec![auth_event];
        events.extend(space_events);

        Ok((groups_y, space_y, auth_message, space_message, events))
    }

    /// Remove a member from the space.
    ///
    /// Returns resulting auth and space state and messages for processing.
    pub async fn remove(
        &self,
        member: impl Into<ActorId>,
    ) -> Result<
        (
            AuthGroupState<C>,
            SpacesState<C>,
            F::Message,
            F::Message,
            Vec<Event<C>>,
        ),
        SpaceError<F, C>,
    > {
        let member = member.into();

        let space_y = self.state().await?;
        let group = Group::new(self.manager.clone(), space_y.group_id);

        let (groups_y, auth_message, auth_event) = group.remove(member).await?;

        let (space_y, space_message, space_events) = Space::process_auth_message(
            self.manager.clone(),
            space_y,
            &SpacesMessage::auth(&auth_message),
        )
        .await?;

        let mut events = vec![auth_event];
        events.extend(space_events);

        Ok((groups_y, space_y, auth_message, space_message, events))
    }

    /// Forge a "pointer" space message from an already existing auth message and apply any
    /// resulting group membership changes. Any resulting encryption direct messages are included
    /// in the space message alongside a reference to the auth message.
    pub(crate) async fn process_auth_message(
        manager_ref: Manager<S, F, C, RS>,
        mut y: SpacesState<C>,
        auth_message: &AuthMessage<C>,
    ) -> Result<(SpacesState<C>, F::Message, Vec<Event<C>>), SpaceError<F, C>> {
        // Get current space members.
        let current_members = secret_members(y.groups_y.members(y.group_id));

        // Process auth message on local auth state.
        y.groups_y = AuthGroup::<C, RS>::process(y.groups_y, auth_message)?;

        // Get next space members.
        let next_members = secret_members(y.groups_y.members(y.group_id));

        // Process the change of membership on encryption the context.
        let (encryption_y, direct_messages) = if current_members != next_members {
            let manager = manager_ref.inner.read().await;
            Self::apply_secret_member_change(
                y.encryption_y,
                auth_message,
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
            // Forge and persist the message.
            manager.identity.forge(args).await?
        };

        y.encryption_y
            .orderer
            .add_dependency(space_message.hash(), &dependencies);

        // If current and next member sets are equal it indicates that the space is not affected
        // by this auth change. This can be because the space wasn't created yet, or the auth
        // change simply does not effect the members of this space. In either case we don't want
        // to emit any membership change event.
        if current_members == next_members {
            return Ok((y, space_message, vec![]));
        };

        // Check if this membership change removes the local actor.
        let me = manager_ref.id();
        let ejected = current_members.contains(&me) && !next_members.contains(&me);

        // Construct space membership event.
        let space_event = space_message_to_space_event(
            y.space_id,
            &SpacesMessage::space_membership(&space_message),
            auth_message,
            current_members,
            next_members,
        );

        let mut events = vec![space_event];
        if ejected {
            events.push(Event::Space(SpaceEvent::Ejected {
                space_id: y.space_id,
            }))
        }

        Ok((y, space_message, events))
    }

    /// Instantiate space state from existing global auth state.
    ///
    /// Every space contains pointers to all messages published to the global auth state. This
    /// method iterates through all existing auth messages and publishes these pointers to the
    /// space. None of the messages will contain encryption control messages as they were
    /// published before the space existed.
    async fn from_group(
        manager_ref: Manager<S, F, C, RS>,
        space_id: SpaceId,
        group_id: GroupId,
    ) -> Result<(SpacesState<C>, Vec<F::Message>), SpaceError<F, C>> {
        // Instantiate empty space state.
        let mut y = { Self::get_or_init_state(space_id, group_id, manager_ref.clone()).await? };
        let mut messages = vec![];

        // Publish pointers for all operations in the global auth graph. We topologically sort the
        // operations and publish them in this linear order.
        //
        // These won't contain any encryption messages as they were published _before_ the space
        // was created.
        let groups_y = manager_ref.get_groups_state().await?;
        let mut manager = manager_ref.inner.write().await;
        let mut space_dependencies = vec![];
        let operations =
            toposort(&groups_y.inner.graph, None).expect("auth graph does not contain cycles");
        for id in operations {
            let operation = groups_y
                .inner
                .operations
                .get(&id)
                .expect("all auth operations exist");

            let args = SpacesArgs::SpaceMembership {
                space_id: y.space_id,
                group_id: y.group_id,
                auth_message_id: operation.id(),
                direct_messages: vec![],
                space_dependencies: space_dependencies.clone(),
            };
            let message = manager.identity.forge(args).await?;

            y.encryption_y
                .orderer
                .add_dependency(message.hash(), &space_dependencies);

            space_dependencies = vec![message.hash()];
            messages.push(message);
        }
        y.groups_y = groups_y;

        Ok((y, messages))
    }

    /// Handle messages which effect the space membership. Each of these messages contained a
    /// pointer to an auth message and the auth message is required here.
    pub(crate) async fn handle_membership_message(
        &self,
        space_message: &SpaceMembershipMessage,
        auth_message: &AuthMessage<C>,
    ) -> Result<Option<(SpacesState<C>, Vec<Event<C>>)>, SpaceError<F, C>> {
        let SpaceMembershipMessage {
            id,
            group_id,
            space_dependencies,
            ..
        } = space_message;

        // Get space state and current members.
        let mut y = Self::get_or_init_state(self.id, *group_id, self.manager.clone()).await?;

        // If we already processed this message return here.
        if y.encryption_y.orderer.has_seen(*id) {
            debug!(
                space_id = self.id().fmt_short(),
                space_message = space_message.id.fmt_short(),
                "ignore already processed space membership message"
            );
            return Ok(None);
        }

        let duplicate_pointer = y.groups_y.inner.operations.contains_key(&auth_message.id());

        let current_members = secret_members(y.groups_y.members(y.group_id));

        debug!(
            space_id = self.id().fmt_short(),
            "current space members: {:?}",
            current_members
                .iter()
                .map(ShortFormat::fmt_short)
                .collect::<Vec<_>>()
        );

        // Process auth message on space auth state.
        //
        // Skip processing if this auth message has already been processed. This can happen when
        // multiple peers concurrently publish pointers to some auth message into the space.
        let next_members = if !duplicate_pointer {
            y.groups_y = AuthGroup::<C, RS>::process(y.groups_y, auth_message)?;
            // Get next space members.
            secret_members(y.groups_y.members(y.group_id))
        } else {
            debug!(
                space_id = self.id().fmt_short(),
                groups_message = auth_message.id().fmt_short(),
                space_message = space_message.id.fmt_short(),
                "ignoring group change already applied to space"
            );
            current_members.clone()
        };

        debug!(
            space_id = self.id().fmt_short(),
            "next space members: {:?}",
            next_members
                .iter()
                .map(ShortFormat::fmt_short)
                .collect::<Vec<_>>()
        );

        // Make the dgm aware of the new space members.
        y.encryption_y.dcgka.dgm.members = next_members.clone();

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
            EncryptionGroup::receive(y.encryption_y, &encryption_message)?;

        y.encryption_y = encryption_y;

        {
            y.encryption_y
                .orderer
                .add_dependency(*id, space_dependencies);
        }

        let events = if !duplicate_pointer {
            let mut events = encryption_output_to_space_events(&self.id(), encryption_output);

            // If current and next member sets are equal it indicates that the space is not affected
            // by this auth change. This can be because the space wasn't created yet, or the auth
            // change simply does not effect the members of this space. In either case we don't want
            // to emit any membership change event.
            if current_members == next_members {
                return Ok(Some((y, events)));
            };

            // Construct space membership event.
            let membership_event = space_message_to_space_event(
                self.id(),
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

        Ok(Some((y, events)))
    }

    /// Apply a group membership change to the group encryption state.
    ///
    /// For "add" and "remove" actions, the difference between the current and next secret group
    /// members (those with "read" access) is computed and only the diff processed on the
    /// encryption group.
    async fn apply_secret_member_change(
        mut encryption_y: EncryptionGroupState,
        auth_message: &AuthMessage<C>,
        current_members: Vec<ActorId>,
        next_members: Vec<ActorId>,
        rng: &Rng,
    ) -> Result<(EncryptionGroupState, Vec<EncryptionDirectMessage>), SpaceError<F, C>> {
        // Make the DGM aware of group members after this group membership change has been
        // processed.
        encryption_y.dcgka.dgm = EncryptionMembershipState {
            members: next_members.clone(),
        };

        let mut direct_messages = vec![];
        let encryption_y = {
            match &auth_message.action() {
                AuthGroupAction::Create { .. } => {
                    let (encryption_y, message) =
                        EncryptionGroup::create(encryption_y, next_members.clone(), rng)?;
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
                            EncryptionGroup::add(encryption_y, added, rng)?;
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
                            EncryptionGroup::remove(encryption_y, removed, rng)?;
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
    pub(crate) async fn handle_application_message(
        &self,
        message: &ApplicationMessage,
    ) -> Result<Option<(SpacesState<C>, Vec<Event<C>>)>, SpaceError<F, C>> {
        let mut y = self.state().await?;

        // Process encryption message.
        let encryption_message = EncryptionMessage::from_application(message);
        let (encryption_y, encryption_output) =
            { EncryptionGroup::receive(y.encryption_y, &encryption_message)? };

        y.encryption_y = encryption_y;

        // Update dependencies.
        y.encryption_y
            .orderer
            .add_dependency(encryption_message.id(), &message.space_dependencies);

        // Persist new state.
        let events = encryption_output_to_space_events(&y.space_id, encryption_output);

        Ok(Some((y, events)))
    }

    pub async fn repair(
        &self,
    ) -> Result<(SpacesState<C>, Vec<F::Message>, Vec<Event<C>>), SpaceError<F, C>> {
        let groups_y = self.manager.get_groups_state().await?;
        let mut space_y = self.state().await?;

        // @TODO: here we need to account for the new Groups::heads_filtered(..) approach to
        // calculating dependencies and only include the ones strictly necessary for this space.
        let operation_ids =
            toposort(&groups_y.inner.graph, None).expect("auth graph does not contain cycles");

        let mut messages = vec![];
        let mut events = vec![];
        // @TODO: we can optimize here by calculating the diff between the current space auth
        // graph tips and the global auth graph tips. Then we could apply only the missing
        // operations rather than applying all operations as we do here.
        for id in operation_ids {
            // This auth message has already been processed by the space.
            if space_y.groups_y.inner.operations.contains_key(&id) {
                continue;
            };

            let message = {
                let manager = self.manager.inner.read().await;
                manager
                    .store
                    .get_spaces_message(&id)
                    .await
                    .map_err(|err| StoreError::MessageStore(err.to_string()))?
                    .expect("message present in store")
            };

            let (space_y_i, space_message, space_events) = Space::process_auth_message(
                self.manager.clone(),
                space_y,
                &SpacesMessage::auth(&message),
            )
            .await?;

            space_y = space_y_i;

            messages.push(space_message);
            events.extend(space_events);
        }

        Ok((space_y, messages, events))
    }

    /// Get the space state.
    pub(crate) async fn state(&self) -> Result<SpacesState<C>, SpaceError<F, C>> {
        let space_y = self
            .manager
            .get_space_state(&self.id)
            .await?
            .ok_or(SpaceError::UnknownSpace(self.id))?;

        // TODO: is there a better way to do this? It seems updating the key material on the space
        // when it changes would be prefered. Inject latest key material to space DCGKA state.
        let manager = self.manager.inner.read().await;

        let key_manager_y = manager.identity.key_manager().await?;
        let key_registry_y = manager.identity.key_registry().await?;

        Ok(SpacesState::assemble_from_store(
            space_y,
            key_manager_y,
            key_registry_y,
        ))
    }

    /// Get or if not present initialize a new space state.
    async fn get_or_init_state(
        space_id: SpaceId,
        group_id: GroupId,
        manager_ref: Manager<S, F, C, RS>,
    ) -> Result<SpacesState<C>, SpaceError<F, C>> {
        let (key_manager_y, key_registry_y) = {
            let manager = manager_ref.inner.read().await;
            let key_manager_y = manager.identity.key_manager().await?;
            let key_registry_y = manager.identity.key_registry().await?;
            (key_manager_y, key_registry_y)
        };

        let result = manager_ref.get_space_state(&space_id).await?;

        let space_y = match result {
            Some(y) => {
                // Inject latest key material to space DCGKA state.
                SpacesState::assemble_from_store(y, key_manager_y, key_registry_y)
            }
            None => {
                let manager = manager_ref.inner.read().await;
                let my_id = manager.identity.id();

                let groups_y = AuthGroupState::new();

                let dgm = EncryptionMembershipState {
                    members: Vec::new(),
                };

                // Encryption orderer state is empty when we're initializing a new encryption
                // state.
                let encryption_orderer_y = EncryptionOrdererState::new();
                let encryption_y = EncryptionGroup::init(
                    my_id,
                    key_manager_y,
                    key_registry_y,
                    dgm,
                    encryption_orderer_y,
                );

                SpacesState {
                    space_id,
                    group_id,
                    groups_y,
                    encryption_y,
                }
            }
        };

        Ok(space_y)
    }

    /// Id of this space.
    pub fn id(&self) -> SpaceId {
        self.id
    }

    /// Id of the group associated with this space.
    pub async fn group_id(&self) -> Result<GroupId, SpaceError<F, C>> {
        let y = self.state().await?;
        Ok(y.group_id)
    }

    /// The members of this space.
    pub async fn members(&self) -> Result<Vec<(MemberId, Access<C>)>, SpaceError<F, C>> {
        let y = self.state().await?;
        let mut group_members = y.groups_y.members(y.group_id);
        sort_members(&mut group_members);
        Ok(group_members)
    }

    /// Publish a message encrypted towards all current group members.
    pub async fn publish(
        &self,
        plaintext: &[u8],
    ) -> Result<(SpacesState<C>, F::Message, Event<C>), SpaceError<F, C>> {
        let mut y = self.state().await?;

        if !y.encryption_y.orderer.is_welcomed() {
            return Err(SpaceError::NotWelcomed(self.id()));
        }

        let mut manager = self.manager.inner.write().await;

        // Encrypt plaintext towards encryption group members.
        let (encryption_y, encryption_args) =
            EncryptionGroup::send(y.encryption_y, plaintext, &manager.rng)?;
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
            .add_dependency(message.hash(), &dependencies);

        let event = Event::Application {
            space_id: y.space_id,
            data: plaintext.to_owned(),
        };

        Ok((y, message, event))
    }
}

#[cfg(any(test, feature = "test_utils"))]
impl<S, F, C, RS> Space<S, F, C, RS>
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
    RS: AuthResolver<C>,
{
    /// Add a member to the space with assigned access level.
    ///
    /// Persists resulting state and returns forged message.
    pub async fn add_persisted(
        &self,
        member: ActorId,
        access: Access<C>,
    ) -> Result<(F::Message, F::Message, Vec<Event<C>>), SpaceError<F, C>> {
        let (groups_y, space_y, auth_message, space_message, events) =
            self.add(member, access).await?;

        self.manager.set_groups_state(&groups_y).await?;
        self.manager
            .set_space_state(&self.id(), &space_y.into())
            .await?;

        Ok((auth_message, space_message, events))
    }

    /// Remove a member from the space.
    ///
    /// Persists resulting state and returns forged message.
    pub async fn remove_persisted(
        &self,
        member: ActorId,
    ) -> Result<(F::Message, F::Message, Vec<Event<C>>), SpaceError<F, C>> {
        let (groups_y, space_y, auth_message, space_message, events) = self.remove(member).await?;

        self.manager.set_groups_state(&groups_y).await?;
        self.manager
            .set_space_state(&self.id(), &space_y.into())
            .await?;

        Ok((auth_message, space_message, events))
    }

    /// Publish a message encrypted towards all current group members.
    pub async fn publish_persisted(
        &self,
        plaintext: &[u8],
    ) -> Result<(F::Message, Event<C>), SpaceError<F, C>> {
        let (space_y, message, event) = self.publish(plaintext).await?;
        self.manager
            .set_space_state(&self.id(), &space_y.into())
            .await?;
        Ok((message, event))
    }

    pub async fn repair_persisted(
        &self,
    ) -> Result<(Vec<F::Message>, Vec<Event<C>>), SpaceError<F, C>> {
        let (space_y, messages, events) = self.repair().await?;
        self.manager
            .set_space_state(&self.id(), &space_y.into())
            .await?;
        Ok((messages, events))
    }
}

/// Space state object.
#[derive(Debug)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct SpacesState<C> {
    pub space_id: SpaceId,
    pub group_id: VerifyingKey,
    pub groups_y: AuthGroupState<C>,
    pub encryption_y: EncryptionGroupState,
}

impl<C> SpacesState<C>
where
    C: Conditions,
{
    pub fn assemble_from_store(
        space_y: SpacesStoreState<C>,
        key_manager_y: KeyManagerState,
        key_registry_y: KeyRegistryState<MemberId>,
    ) -> Self {
        let space_id = space_y.space_id;
        let group_id = space_y.group_id;
        let (groups_y, encryption_y) =
            space_y.assemble_encryption_state(key_manager_y, key_registry_y);

        Self {
            space_id,
            group_id,
            groups_y,
            encryption_y,
        }
    }
}

/// Space error type.
#[derive(Debug, Error)]
pub enum SpaceError<F, C>
where
    F: Forge<C>,
    C: Conditions,
{
    #[error(transparent)]
    Rng(#[from] RngError),

    #[error(transparent)]
    AuthGroup(#[from] AuthGroupError),

    #[error(transparent)]
    Group(#[from] GroupError<F, C>),

    #[error("{0}")]
    EncryptionGroup(#[from] EncryptionGroupError),

    #[error(transparent)]
    IdentityManager(#[from] IdentityError<F, C>),

    #[error(transparent)]
    Store(#[from] StoreError),

    #[error("tried to access unknown space id {0}")]
    UnknownSpace(SpaceId),

    #[error("tried to publish when not a member of space {0}")]
    NotWelcomed(SpaceId),
}
