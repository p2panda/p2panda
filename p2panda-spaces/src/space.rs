// SPDX-License-Identifier: MIT OR Apache-2.0

//! API for managing members of a space and sending/receiving messages.
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::fmt::{self, Debug};
use std::marker::PhantomData;

use p2panda_auth::Access;
use p2panda_auth::traits::{Conditions, Operation};
use p2panda_core::traits::Digest;
use p2panda_core::{Hash, SigningKey, VerifyingKey};
use p2panda_encryption::data_scheme::SecretBundleState;
use p2panda_encryption::data_scheme::dcgka::DcgkaState;
use p2panda_encryption::key_bundle::LongTermKeyBundle;
use p2panda_encryption::key_manager::KeyManager;
use p2panda_encryption::key_registry::KeyRegistry;
use p2panda_encryption::traits::GroupMessage;
use p2panda_encryption::two_party::TwoPartyState;
use p2panda_encryption::{Rng, RngError};
use p2panda_store::Transaction;
use p2panda_store::groups::GroupsStore;
use p2panda_store::key_registry::KeyRegistryStore;
use p2panda_store::key_secrets::KeySecretsStore;
use p2panda_store::spaces::{SpacesMessageStore, SpacesStore, SpacesStoreWrite};
use petgraph::algo::toposort;
use serde::de::{Deserialize, Error as SerdeError, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeSeq};
use thiserror::Error;

use crate::StoreError;
use crate::auth::message::AuthMessage;
use crate::encryption::dgm::EncryptionMembershipState;
use crate::encryption::message::{EncryptionArgs, EncryptionMessage};
use crate::encryption::orderer::EncryptionOrdererState;
use crate::event::{Event, encryption_output_to_space_events, space_message_to_space_event};
use crate::forge::Forge;
use crate::group::{Group, GroupError};
use crate::identity::IdentityError;
use crate::manager::Manager;
use crate::message::{ApplicationMessage, SpaceMembershipMessage, SpacesArgs, SpacesMessage};
use crate::types::{
    AuthGroup, AuthGroupAction, AuthGroupError, AuthGroupState, AuthResolver,
    EncryptionDirectMessage, EncryptionGroup, EncryptionGroupError, EncryptionGroupState,
};
use crate::utils::{added_members, removed_members, secret_members, sort_members};

pub type SpaceId = Hash;

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
        + Transaction
        + SpacesStore<SpaceState<C>>
        + SpacesStoreWrite<SpaceState<C>>
        + SpacesMessageStore<SpacesArgs<C>>
        + GroupsStore<AuthMessage<C>, C>
        + KeyRegistryStore
        + KeySecretsStore,
    F: Forge<C>,
    C: Conditions,
    RS: AuthResolver<C>,
{
    pub(crate) fn new(manager_ref: Manager<S, F, C, RS>, id: SpaceId) -> Self {
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
    /// Returns resulting auth and space state and messages for processing.
    pub(crate) async fn create(
        manager_ref: Manager<S, F, C, RS>,
        space_id: SpaceId,
        mut initial_members: Vec<(VerifyingKey, Access<C>)>,
    ) -> Result<(AuthGroupState<C>, SpaceState<C>, Vec<F::Message>), SpaceError<F, C, RS>> {
        let my_id = manager_ref.id();

        // Automatically add ourselves with "manage" level without any conditions as default.
        if !initial_members.iter().any(|(member, _)| *member == my_id) {
            initial_members.push((my_id, Access::manage()));
        }

        // Generate random group id.
        let group_id = {
            let manager = manager_ref.inner.read().await;
            let signing_key = SigningKey::from_bytes(&manager.rng.random_array()?);
            signing_key.verifying_key()
        };

        // Instantiate new space state from existing global auth state.
        let (space_y, space_history) =
            Self::from_group(manager_ref.clone(), space_id, group_id).await?;

        // Create the space group.
        let auth_y = manager_ref.groups_state().await?;
        let (auth_y, create_group) =
            Group::create(manager_ref.clone(), auth_y, group_id, initial_members)
                .await
                .map_err(SpaceError::Group)?;

        // Apply the "create" auth message to the space state.
        let (space_y, create_space) = Space::process_auth_message(
            manager_ref.clone(),
            space_y,
            &SpacesMessage::auth(&create_group),
        )
        .await?;

        let mut messages = vec![create_group];
        messages.extend(space_history);
        messages.extend([create_space]);

        Ok((auth_y, space_y, messages))
    }

    /// Add a member to the space with assigned access level.
    ///
    /// Persists resulting state and returns forged message.
    #[cfg(any(test, feature = "test_utils"))]
    pub async fn add_persisted(
        &self,
        member: VerifyingKey,
        access: Access<C>,
    ) -> Result<(F::Message, F::Message), SpaceError<F, C, RS>> {
        let (auth_y, space_y, auth_message, space_message) = self.add(member, access).await?;

        self.manager
            .set_groups_state(&auth_y)
            .await
            .map_err(|err| StoreError::from(err.to_string()))?;
        Self::set_state(self.manager.clone(), self.id(), space_y).await?;

        Ok((auth_message, space_message))
    }

    /// Add a member to the space with assigned access level.
    ///
    /// Returns resulting auth and space state and messages for processing.
    pub async fn add(
        &self,
        member: VerifyingKey,
        access: Access<C>,
    ) -> Result<(AuthGroupState<C>, SpaceState<C>, F::Message, F::Message), SpaceError<F, C, RS>>
    {
        let space_y = self.state().await?;
        let group = Group::new(self.manager.clone(), space_y.group_id);
        let (auth_y, auth_message) = group.add(member, access).await.map_err(SpaceError::Group)?;

        let (space_y, space_message) = Space::process_auth_message(
            self.manager.clone(),
            space_y,
            &SpacesMessage::auth(&auth_message),
        )
        .await?;

        Ok((auth_y, space_y, auth_message, space_message))
    }

    /// Remove a member from the space.
    ///
    /// Persists resulting state and returns forged message.
    #[cfg(any(test, feature = "test_utils"))]
    pub async fn remove_persisted(
        &self,
        member: VerifyingKey,
    ) -> Result<(F::Message, F::Message), SpaceError<F, C, RS>> {
        let (auth_y, space_y, auth_message, space_message) = self.remove(member).await?;

        self.manager.set_groups_state(&auth_y).await?;
        Self::set_state(self.manager.clone(), self.id(), space_y).await?;

        Ok((auth_message, space_message))
    }

    /// Remove a member from the space.
    ///
    /// Returns resulting auth and space state and messages for processing.
    pub async fn remove(
        &self,
        member: VerifyingKey,
    ) -> Result<(AuthGroupState<C>, SpaceState<C>, F::Message, F::Message), SpaceError<F, C, RS>>
    {
        let space_y = self.state().await?;
        let group = Group::new(self.manager.clone(), space_y.group_id);
        let (auth_y, auth_message) = group.remove(member).await.map_err(SpaceError::Group)?;

        let (space_y, space_message) = Space::process_auth_message(
            self.manager.clone(),
            space_y,
            &SpacesMessage::auth(&auth_message),
        )
        .await?;

        Ok((auth_y, space_y, auth_message, space_message))
    }

    /// Forge a "pointer" space message from an already existing auth message and apply any
    /// resulting group membership changes. Any resulting encryption direct messages are included
    /// in the space message alongside a reference to the auth message.
    pub(crate) async fn process_auth_message(
        manager_ref: Manager<S, F, C, RS>,
        mut y: SpaceState<C>,
        auth_message: &AuthMessage<C>,
    ) -> Result<(SpaceState<C>, F::Message), SpaceError<F, C, RS>> {
        // Get current space members.
        let current_members = secret_members(y.auth_y.members(y.group_id));

        // Process auth message on local auth state.
        y.auth_y = AuthGroup::process(y.auth_y, auth_message).map_err(SpaceError::AuthGroup)?;

        // Get next space members.
        let next_members = secret_members(y.auth_y.members(y.group_id));

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
        let dependencies: Vec<Hash> = y.encryption_y.orderer.heads().to_vec();
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

        Ok((y, space_message))
    }

    /// Instantiate space state from existing global auth state.
    ///
    /// Every space contains pointers to all messages published to the global auth state. This
    /// method iterates through all existing auth messages and publishes these pointers to the
    /// space. None of the messages will contain encryption control messages as they were published
    /// before the space existed.
    async fn from_group(
        manager_ref: Manager<S, F, C, RS>,
        space_id: SpaceId,
        group_id: VerifyingKey,
    ) -> Result<(SpaceState<C>, Vec<F::Message>), SpaceError<F, C, RS>> {
        // Instantiate empty space state.
        let mut y = { Self::get_or_init_state(space_id, group_id, manager_ref.clone()).await? };
        let mut messages = vec![];

        // Publish pointers for all operations in the global auth graph. We topologically sort the
        // operations and publish them in this linear order.
        //
        // These won't contain any encryption messages as they were published _before_ the space was
        // created.
        let auth_y = manager_ref.groups_state().await?;
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

            space_dependencies = vec![message.hash()];
            messages.push(message);
        }
        y.auth_y = auth_y;

        Ok((y, messages))
    }

    /// Handle messages which effect the space membership. Each of these messages contained a
    /// pointer to an auth message and the auth message is required here.
    pub(crate) async fn handle_membership_message(
        &self,
        space_message: &SpaceMembershipMessage,
        auth_message: &AuthMessage<C>,
    ) -> Result<Vec<Event<C>>, SpaceError<F, C, RS>> {
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
            y.encryption_y
                .orderer
                .add_dependency(*id, space_dependencies);
            self.manager
                .set_space_state(&self.id, &y)
                .await
                .map_err(|err| StoreError::from(err.to_string()))?;
        }

        let events = if !duplicate_pointer {
            let mut events = encryption_output_to_space_events(&self.id(), encryption_output);

            // If current and next member sets are equal it indicates that the space is not affected
            // by this auth change. This can be because the space wasn't created yet, or the auth
            // change simply does not effect the members of this space. In either case we don't want
            // to emit any membership change event.
            if current_members == next_members {
                return Ok(events);
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

        Ok(events)
    }

    /// Apply a group membership change to the group encryption state.
    ///
    /// For "add" and "remove" actions, the difference between the current and next secret group
    /// members (those with "read" access) is computed and only the diff processed on the
    /// encryption group.
    async fn apply_secret_member_change(
        mut encryption_y: EncryptionGroupState,
        auth_message: &AuthMessage<C>,
        current_members: Vec<VerifyingKey>,
        next_members: Vec<VerifyingKey>,
        rng: &Rng,
    ) -> Result<(EncryptionGroupState, Vec<EncryptionDirectMessage>), SpaceError<F, C, RS>> {
        // Make the DGM aware of group members after this group membership change has been
        // processed.
        encryption_y.dcgka.dgm = EncryptionMembershipState {
            members: HashSet::from_iter(next_members.clone()),
        };

        let mut direct_messages = vec![];
        let encryption_y = {
            match &auth_message.action() {
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
    pub(crate) async fn handle_application_message(
        &self,
        message: &ApplicationMessage,
    ) -> Result<Vec<Event<C>>, SpaceError<F, C, RS>> {
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
            .add_dependency(encryption_message.id(), &message.space_dependencies);

        // Persist new state.
        let events = encryption_output_to_space_events(&y.space_id, encryption_output);
        self.manager
            .set_space_state(&self.id, &y)
            .await
            .map_err(|err| StoreError::from(err.to_string()))?;

        Ok(events)
    }

    pub async fn repair(&self) -> Result<Vec<F::Message>, SpaceError<F, C, RS>> {
        let global_auth_y = self.manager.groups_state().await?;

        // TODO: here we need to account for the new Groups::heads_filtered(..) approach to
        // calculating dependencies and only include the ones strictly necessary for this space.
        let operation_ids =
            toposort(&global_auth_y.inner.graph, None).expect("auth graph does not contain cycles");

        let mut messages = vec![];
        // TODO: we can optimize here by calculating the diff between the current space auth graph
        // tips and the global auth graph tips. Then we could apply only the missing operations
        // rather than applying all operations as we do here.
        for id in operation_ids {
            // This auth message has already been processed by the space.
            let y = self.state().await?;
            if y.auth_y.inner.operations.contains_key(&id) {
                continue;
            };

            let message = {
                let manager = self.manager.inner.read().await;
                manager
                    .store
                    .get_spaces_message(&id)
                    .await
                    .map_err(|err| StoreError::from(err.to_string()))?
                    .expect("message present in store")
            };

            let (y, space_message) = Space::process_auth_message(
                self.manager.clone(),
                y,
                &SpacesMessage::auth(&message),
            )
            .await?;
            Space::set_state(self.manager.clone(), self.id(), y).await?;

            messages.push(space_message);
        }

        Ok(messages)
    }

    /// Get the space state.
    pub(crate) async fn state(&self) -> Result<SpaceState<C>, SpaceError<F, C, RS>> {
        let mut space_y = self
            .manager
            .space_state(self.id)
            .await
            .map_err(|err| StoreError::from(err.to_string()))?
            .ok_or(SpaceError::UnknownSpace(self.id))?;

        // Inject latest key material to space DCGKA state.
        let manager = self.manager.inner.read().await;
        let key_manager_y = manager.identity.key_manager().await?;
        let key_registry_y = manager.identity.key_registry().await?;

        space_y.encryption_y.dcgka.my_keys = key_manager_y;
        space_y.encryption_y.dcgka.pki = key_registry_y;

        Ok(space_y)
    }

    /// Get or if not present initialize a new space state.
    async fn get_or_init_state(
        space_id: SpaceId,
        group_id: VerifyingKey,
        manager_ref: Manager<S, F, C, RS>,
    ) -> Result<SpaceState<C>, SpaceError<F, C, RS>> {
        let manager = manager_ref.inner.read().await;

        let key_manager_y = manager.identity.key_manager().await?;
        let key_registry_y = manager.identity.key_registry().await?;

        let result = manager_ref
            .space_state(space_id)
            .await
            .map_err(|err| StoreError::from(err.to_string()))?;

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
                let encryption_y = EncryptionGroup::init(
                    my_id,
                    key_manager_y,
                    key_registry_y,
                    dgm,
                    encryption_orderer_y,
                );

                SpaceState::from_state(space_id, group_id, AuthGroupState::new(), encryption_y)
            }
        };

        // TODO: This is ugly, improve space initialization code so that we don't have to pass in
        // the group id like we do now.
        //
        // Sanity check.
        assert_eq!(space_y.group_id, group_id);
        Ok(space_y)
    }

    async fn set_state(
        manager_ref: Manager<S, F, C, RS>,
        space_id: SpaceId,
        y: SpaceState<C>,
    ) -> Result<(), SpaceError<F, C, RS>> {
        manager_ref
            .set_space_state(&space_id, &y)
            .await
            .map_err(|err| StoreError::from(err.to_string()))?;
        Ok(())
    }

    /// Id of this space.
    pub fn id(&self) -> SpaceId {
        self.id
    }

    /// Id of the group associated with this space.
    pub async fn group_id(&self) -> Result<VerifyingKey, SpaceError<F, C, RS>> {
        let y = self.state().await?;
        Ok(y.group_id)
    }

    /// The members of this space.
    pub async fn members(&self) -> Result<Vec<(VerifyingKey, Access<C>)>, SpaceError<F, C, RS>> {
        let y = self.state().await?;
        let mut group_members = y.auth_y.members(y.group_id);
        sort_members(&mut group_members);
        Ok(group_members)
    }

    /// Publish a message encrypted towards all current group members.
    pub async fn publish(&self, plaintext: &[u8]) -> Result<F::Message, SpaceError<F, C, RS>> {
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
            .add_dependency(message.hash(), &dependencies);

        drop(manager);

        // Persist space state.
        self.manager
            .set_space_state(&self.id, &y)
            .await
            .map_err(|err| StoreError::from(err.to_string()))?;

        Ok(message)
    }
}

/// Space state object.
// TODO: This is what gets stored in the database and we want to have a closer look how that
// actually looks like & we can do better than just "dumping" everything into it.
//
// Probably we want a custom struct here and a higher-level method which assembles all the pieces
// together to the final state (as we already do for example for EncryptionGroupState).
#[derive(Debug)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct SpaceState<C>
where
    C: Conditions,
{
    pub space_id: SpaceId,
    pub group_id: VerifyingKey,
    pub auth_y: AuthGroupState<C>,
    pub encryption_y: EncryptionGroupState,
}

impl<C> SpaceState<C>
where
    C: Conditions,
{
    pub fn from_state(
        space_id: SpaceId,
        group_id: VerifyingKey,
        auth_y: AuthGroupState<C>,
        encryption_y: EncryptionGroupState,
    ) -> Self {
        Self {
            space_id,
            group_id,
            auth_y,
            encryption_y,
        }
    }
}

impl<C> Serialize for SpaceState<C>
where
    C: Conditions + Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(8))?;

        seq.serialize_element(&self.space_id)?;
        seq.serialize_element(&self.group_id)?;
        seq.serialize_element(&self.auth_y)?;

        // TODO: Check if there's more things we can remove from serialized data.
        seq.serialize_element(&self.encryption_y.my_id)?;
        seq.serialize_element(&self.encryption_y.is_welcomed)?;
        seq.serialize_element(&self.encryption_y.secrets)?;
        seq.serialize_element(&self.encryption_y.orderer)?;
        seq.serialize_element(&self.encryption_y.dcgka.two_party)?;

        seq.end()
    }
}

impl<'de, C> Deserialize<'de> for SpaceState<C>
where
    C: Conditions + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SpaceStateVisitor<C> {
            _marker: PhantomData<C>,
        }

        impl<'de, C> Visitor<'de> for SpaceStateVisitor<C>
        where
            C: Conditions + Deserialize<'de>,
        {
            type Value = SpaceState<C>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("space state encoded as a sequence")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let space_id: SpaceId = seq
                    .next_element()?
                    .ok_or(SerdeError::custom("space id missing"))?;

                let group_id: VerifyingKey = seq
                    .next_element()?
                    .ok_or(SerdeError::custom("group id missing"))?;

                let auth_y: AuthGroupState<C> = seq
                    .next_element()?
                    .ok_or(SerdeError::custom("group state missing"))?;

                let my_id: VerifyingKey = seq
                    .next_element()?
                    .ok_or(SerdeError::custom("my actor id missing"))?;

                let is_welcomed: bool = seq
                    .next_element()?
                    .ok_or(SerdeError::custom("welcomed state missing"))?;

                let secrets: SecretBundleState = seq
                    .next_element()?
                    .ok_or(SerdeError::custom("secret bundle state missing"))?;

                let orderer: EncryptionOrdererState = seq
                    .next_element()?
                    .ok_or(SerdeError::custom("encryption orderer state missing"))?;

                let two_party: HashMap<VerifyingKey, TwoPartyState<LongTermKeyBundle>> = seq
                    .next_element()?
                    .ok_or(SerdeError::custom("encryption orderer state missing"))?;

                let encryption_y = EncryptionGroupState {
                    my_id,
                    dcgka: DcgkaState {
                        // pki and my_keys will be replaced by state from stores.
                        pki: KeyRegistry::init(),
                        // TODO: This requires encryption test_utils currently, ideally we just
                        // don't use EncryptionMembershipState on this level but a custom struct.
                        my_keys: KeyManager::init(
                            &p2panda_encryption::crypto::x25519::SecretKey::from_bytes([0; 32]),
                        )
                        .expect("hard-coded values"),
                        my_id,
                        two_party,
                        dgm: EncryptionMembershipState::default(),
                    },
                    orderer,
                    secrets,
                    is_welcomed,
                };

                Ok(SpaceState {
                    space_id,
                    group_id,
                    auth_y,
                    encryption_y,
                })
            }
        }

        deserializer.deserialize_seq(SpaceStateVisitor::<C> {
            _marker: PhantomData,
        })
    }
}

/// Space error type.
#[derive(Debug, Error)]
pub enum SpaceError<F, C, RS>
where
    F: Forge<C>,
    C: Conditions,
    RS: AuthResolver<C>,
{
    #[error(transparent)]
    Rng(#[from] RngError),

    #[error("{0}")]
    AuthGroup(AuthGroupError<C, RS>),

    #[error("{0}")]
    Group(GroupError<F, C, RS>),

    #[error("{0}")]
    EncryptionGroup(EncryptionGroupError),

    #[error(transparent)]
    IdentityManager(#[from] IdentityError<F, C>),

    #[error(transparent)]
    Store(#[from] StoreError),

    #[error("{0}")]
    EncryptionOrderer(Infallible),

    #[error("tried to access unknown space id {0}")]
    UnknownSpace(SpaceId),

    #[error("tried to publish when not a member of space {0}")]
    NotWelcomed(SpaceId),
}
