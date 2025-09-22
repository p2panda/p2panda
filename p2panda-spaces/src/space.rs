// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::convert::Infallible;
use std::fmt::Debug;

use p2panda_auth::Access;
use p2panda_auth::traits::{Conditions, Operation};
use p2panda_encryption::{Rng, RngError};
use petgraph::algo::toposort;
use thiserror::Error;

use crate::auth::message::AuthMessage;
use crate::auth::orderer::AuthOrdererState;
use crate::encryption::dgm::EncryptionMembershipState;
use crate::encryption::message::{EncryptionArgs, EncryptionMessage};
use crate::encryption::orderer::EncryptionOrdererState;
use crate::event::Event;
use crate::forge::Forge;
use crate::group::{Group, GroupError};
use crate::manager::Manager;
use crate::message::{AuthoredMessage, SpaceMembershipControlMessage, SpacesArgs, SpacesMessage};
use crate::store::{AuthStore, KeyStore, MessageStore, SpaceStore};
use crate::traits::SpaceId;
use crate::types::{
    ActorId, AuthGroup, AuthGroupAction, AuthGroupError, AuthGroupState, AuthResolver,
    EncryptionGroup, EncryptionGroupError, EncryptionGroupOutput, EncryptionGroupState,
    OperationId,
};

/// Encrypted data context with authorization boundary.
///
/// Only members with suitable access to the space can read and write to it.
#[derive(Debug)]
pub struct Space<ID, S, F, M, C, RS> {
    /// Reference to the manager.
    ///
    /// This allows us build an API where users can treat "space" instances independently from the
    /// manager API, even though internally it has a reference to it.
    manager: Manager<ID, S, F, M, C, RS>,

    /// Id of the space.
    ///
    /// This is the "pointer" at the related space state which lives inside the manager.
    id: ID,
}

impl<ID, S, F, M, C, RS> Space<ID, S, F, M, C, RS>
where
    ID: SpaceId,
    S: SpaceStore<ID, M, C> + KeyStore + AuthStore<C> + MessageStore<M>,
    F: Forge<ID, M, C>,
    M: AuthoredMessage + SpacesMessage<ID, C>,
    C: Conditions,
    RS: Debug + AuthResolver<C>,
{
    pub(crate) fn new(manager_ref: Manager<ID, S, F, M, C, RS>, id: ID) -> Self {
        Self {
            manager: manager_ref,
            id,
        }
    }

    /// Create a space containing initial members and access levels.
    ///
    /// If not already included, then the local actor (creator of this space) will be added to the
    /// initial members and given manage access level.
    pub(crate) async fn create(
        manager_ref: Manager<ID, S, F, M, C, RS>,
        space_id: ID,
        mut initial_members: Vec<(ActorId, Access<C>)>,
    ) -> Result<(Self, Vec<M>), SpaceError<ID, S, F, M, C, RS>> {
        let my_id: ActorId = {
            let manager = manager_ref.inner.read().await;
            manager.forge.public_key().into()
        };

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
        let (group, mut messages) = Group::create(manager_ref.clone(), initial_members)
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
        let space_message =
            Self::process_auth_message(manager_ref.clone(), y, &messages[0]).await?;

        // Push the message for the newly created space to the messages vec.
        messages.push(space_message);

        Ok((
            Self {
                id: space_id,
                manager: manager_ref,
            },
            messages,
        ))
    }

    /// Add a member to the space with assigned access level.
    pub async fn add(
        &self,
        member: ActorId,
        access: Access<C>,
    ) -> Result<Vec<M>, SpaceError<ID, S, F, M, C, RS>> {
        let y = self.state().await?;

        // If the space exists we can assume the associated group exists.
        let group = Group::new(self.manager.clone(), y.group_id);
        group.add(member, access).await.map_err(SpaceError::Group)
    }

    /// Remove a member from the space.
    pub async fn remove(&self, member: ActorId) -> Result<Vec<M>, SpaceError<ID, S, F, M, C, RS>> {
        let y = self.state().await?;

        // If the space exists we can assume the associated group exists.
        let group = Group::new(self.manager.clone(), y.group_id);
        group.remove(member).await.map_err(SpaceError::Group)
    }

    /// Wrap an already forged auth message in a space message and apply any required group
    /// membership changes to the encryption group context. Any resulting encryption control
    /// messages are published on the space message alongside a reference to the auth message.
    pub(crate) async fn process_auth_message(
        manager_ref: Manager<ID, S, F, M, C, RS>,
        mut y: SpaceState<ID, M, C>,
        auth_message: &M,
    ) -> Result<M, SpaceError<ID, S, F, M, C, RS>> {
        if !y.processed_auth.insert(auth_message.id()) {
            panic!("only un-processed auth messages expected")
        }

        // Get current space members.
        let current_members = secret_members(y.auth_y.members(y.group_id));

        // Process auth message on local auth state.
        let auth_message = AuthMessage::from_forged(auth_message);
        y.auth_y = AuthGroup::process(y.auth_y, &auth_message).map_err(SpaceError::AuthGroup)?;

        // Get next space members.
        let next_members = secret_members(y.auth_y.members(y.group_id));

        // Process the change of membership on encryption the context.
        let (encryption_y, encryption_messages) = if current_members != next_members {
            let manager = manager_ref.inner.read().await;
            Self::apply_secret_member_change(
                y.encryption_y,
                &auth_message,
                current_members,
                next_members,
                &manager.rng,
            )
            .await?
        } else {
            (y.encryption_y, vec![])
        };
        y.encryption_y = encryption_y;

        // Construct space message and sign it in the forge (F)
        let dependencies: Vec<OperationId> = y.encryption_y.orderer.heads().to_vec();
        let space_message = {
            let control_messages = encryption_messages
                .iter()
                .map(SpaceMembershipControlMessage::from_encryption_message)
                .collect();

            let args = SpacesArgs::SpaceMembership {
                space_id: y.space_id,
                group_id: y.group_id,
                space_dependencies: dependencies.clone(),
                auth_message_id: auth_message.id(),
                control_messages,
            };

            let mut manager = manager_ref.inner.write().await;
            let message = manager.forge.forge(args).await.map_err(SpaceError::Forge)?;
            manager
                .store
                .set_message(&message.id(), &message)
                .await
                .map_err(SpaceError::MessageStore)?;

            message
        };

        // Update space state and persist it.
        {
            let mut manager = manager_ref.inner.write().await;
            y.encryption_y
                .orderer
                .add_dependency(space_message.id(), &dependencies);

            let space_id = y.space_id;
            manager
                .store
                .set_space(&space_id, y)
                .await
                .map_err(SpaceError::SpaceStore)?;
        }

        Ok(space_message)
    }

    /// Process a space message along with it's relevant auth message (if required).
    pub(crate) async fn process(
        &self,
        space_message: &M,
        auth_message: Option<&AuthMessage<C>>,
    ) -> Result<Vec<Event<ID>>, SpaceError<ID, S, F, M, C, RS>> {
        let events = match space_message.args() {
            SpacesArgs::KeyBundle {} => unreachable!("can't process key bundles here"),
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
    /// Every space contains a "wrapped" reference to all messages published to the global auth
    /// state. This method iterates through all existing auth messages and re-publishes them
    /// towards this space. None of the messages will contain encryption control messages as they
    /// were published before the space existed.
    async fn state_from_auth(
        manager_ref: Manager<ID, S, F, M, C, RS>,
        auth_y: AuthGroupState<C>,
        space_id: ID,
        group_id: ActorId,
        messages: &mut Vec<M>,
    ) -> Result<SpaceState<ID, M, C>, SpaceError<ID, S, F, M, C, RS>> {
        // Instantiate empty space state.
        let mut y = { Self::get_or_init_state(space_id, group_id, manager_ref.clone()).await? };

        // Publish space messages for all operations in the global auth graph. We topologically
        // sort the operations and publish them in this linear order.
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
                control_messages: vec![],
                space_dependencies,
            };
            let message = manager.forge.forge(args).await.map_err(SpaceError::Forge)?;

            manager
                .store
                .set_message(&message.id(), &message)
                .await
                .map_err(SpaceError::MessageStore)?;

            space_dependencies = vec![message.id()];
            messages.push(message);
        }
        y.auth_y = auth_y;
        Ok(y)
    }

    /// Handle messages which effect the space membership.
    async fn handle_membership_message(
        &self,
        space_message: &M,
        auth_message: &AuthMessage<C>,
    ) -> Result<Vec<Event<ID>>, SpaceError<ID, S, F, M, C, RS>> {
        let SpacesArgs::SpaceMembership {
            group_id,
            space_dependencies,
            auth_message_id,
            ..
        } = space_message.args()
        else {
            panic!("unexpected message type");
        };

        // Sanity check.
        assert_eq!(auth_message.id(), *auth_message_id);

        // Process auth message on local auth state.
        let mut y = Self::get_or_init_state(self.id, *group_id, self.manager.clone()).await?;
        y.auth_y = AuthGroup::process(y.auth_y, auth_message).map_err(SpaceError::AuthGroup)?;
        y.processed_auth.insert(auth_message.id());

        // Process encryption messages.
        let encryption_messages = EncryptionMessage::from_membership(space_message);
        let mut encryption_output = vec![];
        for encryption_message in encryption_messages {
            // Make encryption DGM aware of current auth members state.
            let group_members = y.auth_y.members(y.group_id);
            let secret_members = secret_members(group_members);
            y.encryption_y.dcgka.dgm = EncryptionMembershipState {
                members: HashSet::from_iter(secret_members.into_iter()),
            };

            let (encryption_y, encryption_output_inner) =
                EncryptionGroup::receive(y.encryption_y, &encryption_message)
                    .map_err(SpaceError::EncryptionGroup)?;

            encryption_output.extend(encryption_output_inner);

            y.encryption_y = encryption_y
        }

        // Persist new space state.
        {
            let mut manager = self.manager.inner.write().await;
            y.encryption_y
                .orderer
                .add_dependency(space_message.id(), space_dependencies);
            manager
                .store
                .set_space(&self.id, y)
                .await
                .map_err(SpaceError::SpaceStore)?;
        }
        Ok(encryption_output_to_events(self.id(), encryption_output))
    }

    /// Apply a group membership change to the group encryption state.
    ///
    /// The difference between the current and next secret group members (those with "read"
    /// access) is computed and only the diff processed on the encryption group.
    ///
    /// If it is a group being removed/added to the encryption context, then one encryption
    /// control message for each actor (individual) will be generated.
    async fn apply_secret_member_change(
        mut encryption_y: EncryptionGroupState<M>,
        auth_message: &AuthMessage<C>,
        current_members: Vec<ActorId>,
        next_members: Vec<ActorId>,
        rng: &Rng,
    ) -> Result<(EncryptionGroupState<M>, Vec<EncryptionMessage>), SpaceError<ID, S, F, M, C, RS>>
    {
        // Make the DGM aware of group members after this group membership change has been
        // processed.
        encryption_y.dcgka.dgm = EncryptionMembershipState {
            members: HashSet::from_iter(next_members.clone().into_iter()),
        };

        let mut messages = vec![];
        let encryption_y = {
            match &auth_message.payload().action {
                AuthGroupAction::Create { .. } => {
                    let (encryption_y, message) =
                        EncryptionGroup::create(encryption_y, next_members.clone(), rng)
                            .map_err(SpaceError::EncryptionGroup)?;
                    messages.push(message);
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
                        messages.push(message);
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
                        messages.push(message);
                    }
                    encryption_y
                }
                _ => unimplemented!(),
            }
        };

        Ok((encryption_y, messages))
    }

    /// Handle space application messages.
    async fn handle_application_message(
        &self,
        message: &M,
    ) -> Result<Vec<Event<ID>>, SpaceError<ID, S, F, M, C, RS>> {
        let mut y = self.state().await?;

        // Process encryption message.
        let encryption_message = EncryptionMessage::from_application(message);
        let (encryption_y, encryption_output) = {
            EncryptionGroup::receive(y.encryption_y, &encryption_message)
                .map_err(SpaceError::EncryptionGroup)?
        };

        // @TODO: application messages are not included in the encryption orderer state. We need
        //        to decide what to do with them.

        y.encryption_y = encryption_y;

        // Persist new state.
        let mut manager = self.manager.inner.write().await;
        manager
            .store
            .set_space(&self.id, y)
            .await
            .map_err(SpaceError::SpaceStore)?;

        Ok(encryption_output_to_events(self.id(), encryption_output))
    }

    /// Sync a shared auth state change with this space.
    pub(crate) async fn sync_auth(
        &self,
        auth_message: &M,
    ) -> Result<Option<M>, SpaceError<ID, S, F, M, C, RS>> {
        // If this space already processed this auth message then skip it.
        let y = self.state().await?;
        if y.processed_auth.contains(&auth_message.id()) {
            return Ok(None);
        }

        let my_id = self.manager.id().await;
        let is_reader = self
            .members()
            .await?
            .iter()
            .any(|(member, access)| *member == my_id && access > &Access::pull());

        if is_reader {
            let space_message =
                Space::process_auth_message(self.manager.clone(), y, auth_message).await?;
            return Ok(Some(space_message));
        }

        Ok(None)
    }

    /// Get the space state.
    pub(crate) async fn state(
        &self,
    ) -> Result<SpaceState<ID, M, C>, SpaceError<ID, S, F, M, C, RS>> {
        let manager = self.manager.inner.read().await;
        let mut space_y = manager
            .store
            .space(&self.id)
            .await
            .map_err(SpaceError::SpaceStore)?
            .ok_or(SpaceError::UnknownSpace(self.id))?;

        // Inject latest key material to space DCGKA state.
        let key_manager_y = manager
            .store
            .key_manager()
            .await
            .map_err(SpaceError::KeyStore)?;

        let key_registry_y = manager
            .store
            .key_registry()
            .await
            .map_err(SpaceError::KeyStore)?;

        space_y.encryption_y.dcgka.my_keys = key_manager_y;
        space_y.encryption_y.dcgka.pki = key_registry_y;

        Ok(space_y)
    }

    /// Get or if not present initialize a new space state.
    async fn get_or_init_state(
        space_id: ID,
        group_id: ActorId,
        manager_ref: Manager<ID, S, F, M, C, RS>,
    ) -> Result<SpaceState<ID, M, C>, SpaceError<ID, S, F, M, C, RS>> {
        let manager = manager_ref.inner.read().await;

        let key_manager_y = manager
            .store
            .key_manager()
            .await
            .map_err(SpaceError::KeyStore)?;

        let key_registry_y = manager
            .store
            .key_registry()
            .await
            .map_err(SpaceError::KeyStore)?;

        let result = manager
            .store
            .space(&space_id)
            .await
            .map_err(SpaceError::SpaceStore)?;

        let space_y = match result {
            Some(mut y) => {
                // Inject latest key material to space DCGKA state.
                y.encryption_y.dcgka.my_keys = key_manager_y;
                y.encryption_y.dcgka.pki = key_registry_y;
                y
            }
            None => {
                let my_id: ActorId = manager.forge.public_key().into();

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
    pub async fn group_id(&self) -> Result<ActorId, SpaceError<ID, S, F, M, C, RS>> {
        let y = self.state().await?;
        Ok(y.group_id)
    }

    /// The members of this space.
    pub async fn members(
        &self,
    ) -> Result<Vec<(ActorId, Access<C>)>, SpaceError<ID, S, F, M, C, RS>> {
        let y = self.state().await?;
        let group_members = y.auth_y.members(y.group_id);
        Ok(group_members)
    }

    /// Publish a message encrypted towards all current group members.
    pub async fn publish(&self, plaintext: &[u8]) -> Result<M, SpaceError<ID, S, F, M, C, RS>> {
        let mut y = self.state().await?;

        if !y.encryption_y.orderer.is_welcomed() {
            return Err(SpaceError::NotWelcomed(self.id()));
        }

        let mut manager = self.manager.inner.write().await;

        let (encryption_y, encryption_args) =
            EncryptionGroup::send(y.encryption_y, plaintext, &manager.rng)
                .map_err(SpaceError::EncryptionGroup)?;

        let args = {
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
            SpacesArgs::Application {
                space_id: y.space_id,
                space_dependencies: dependencies,
                group_secret_id,
                nonce,
                ciphertext,
            }
        };

        // @TODO: application messages are not included in the encryption orderer state. We need
        //        to decide what to do with them.

        y.encryption_y = encryption_y;

        manager
            .store
            .set_space(&self.id, y)
            .await
            .map_err(SpaceError::SpaceStore)?;

        let message = manager.forge.forge(args).await.map_err(SpaceError::Forge)?;

        Ok(message)
    }
}

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
    pub processed_auth: HashSet<OperationId>,
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
            processed_auth: HashSet::default(),
        }
    }
}

pub fn secret_members<C>(members: Vec<(ActorId, Access<C>)>) -> Vec<ActorId> {
    members
        .into_iter()
        .filter_map(|(id, access)| if access.is_pull() { None } else { Some(id) })
        .collect()
}

pub fn added_members(current_members: Vec<ActorId>, next_members: Vec<ActorId>) -> Vec<ActorId> {
    next_members
        .iter()
        .cloned()
        .filter(|actor| !current_members.contains(actor))
        .collect::<Vec<_>>()
}

pub fn removed_members(current_members: Vec<ActorId>, next_members: Vec<ActorId>) -> Vec<ActorId> {
    current_members
        .iter()
        .cloned()
        .filter(|actor| !next_members.contains(actor))
        .collect::<Vec<_>>()
}

fn encryption_output_to_events<ID, M>(
    space_id: ID,
    encryption_output: Vec<EncryptionGroupOutput<M>>,
) -> Vec<Event<ID>>
where
    ID: SpaceId,
{
    encryption_output
        .into_iter()
        .map(|event| {
            match event {
                EncryptionGroupOutput::Application { plaintext } => Event::Application {
                    space_id,
                    data: plaintext,
                },
                EncryptionGroupOutput::Removed => Event::Removed { space_id },
                _ => {
                    // We only expect "application" events inside this function.
                    unreachable!();
                }
            }
        })
        .collect()
}

#[derive(Debug, Error)]
pub enum SpaceError<ID, S, F, M, C, RS>
where
    ID: SpaceId,
    S: SpaceStore<ID, M, C> + KeyStore + AuthStore<C> + MessageStore<M>,
    F: Forge<ID, M, C>,
    C: Conditions,
    RS: AuthResolver<C> + Debug,
{
    #[error(transparent)]
    Rng(#[from] RngError),

    #[error("{0}")]
    AuthGroup(AuthGroupError<C, RS>),

    #[error("{0}")]
    Group(GroupError<ID, S, F, M, C, RS>),

    #[error("{0}")]
    EncryptionGroup(EncryptionGroupError<M>),

    #[error("{0}")]
    Forge(F::Error),

    #[error("{0}")]
    AuthStore(<S as AuthStore<C>>::Error),

    #[error("{0}")]
    MessageStore(<S as MessageStore<M>>::Error),

    #[error("{0}")]
    KeyStore(<S as KeyStore>::Error),

    #[error("{0}")]
    SpaceStore(<S as SpaceStore<ID, M, C>>::Error),

    #[error("{0}")]
    EncryptionOrderer(Infallible),

    #[error("tried to access unknown space id {0}")]
    UnknownSpace(ID),

    #[error("tried to publish when not a member of space {0}")]
    NotWelcomed(ID),
}
