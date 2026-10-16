// SPDX-License-Identifier: MIT OR Apache-2.0

//! API for managing members of a space and sending/receiving messages.
use std::collections::HashSet;
use std::fmt::Debug;

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_auth::traits::{Conditions, Operation};
use p2panda_auth::validation::{VerifyClaimedWriteError, verify_claimed_write_access};
use p2panda_core::traits::{Digest, ShortFormat};
use p2panda_core::{SigningKey, VerifyingKey};
use p2panda_encryption::key_manager::KeyManagerState;
use p2panda_encryption::key_registry::KeyRegistryState;
use p2panda_encryption::traits::GroupMessage;
use p2panda_encryption::{Rng, RngError};
use p2panda_store::Transaction;
use p2panda_store::groups::GroupsStore;
use p2panda_store::key_registry::KeyRegistryStore;
use p2panda_store::key_secrets::PreKeyBundlesStore;
use p2panda_store::spaces::{SpacesMessageStore, SpacesStore};
use thiserror::Error;
use tracing::debug;

use crate::encryption::dgm::EncryptionMembershipState;
use crate::encryption::message::{EncryptionArgs, EncryptionMessage};
use crate::event::{Event, encryption_output_to_space_events, to_space_event};
use crate::forge::Forge;
use crate::group::{Group, GroupError, GroupOutput};
use crate::identity::IdentityError;
use crate::manager::{Manager, StoreError};
use crate::message::{
    ApplicationMessage, AuthMessage, SpaceMembershipMessage, SpacesArgs, SpacesMessage,
};
use crate::store::{
    Context, SpaceState, SpacesStoreState, StateChanges, StateError, StateTransaction,
};
use crate::types::{
    AuthGroup, AuthGroupAction, AuthGroupError, AuthGroupState, EncryptionDirectMessage,
    EncryptionGroup, EncryptionGroupError, EncryptionGroupState,
};
use crate::utils::{
    added_secret_members_diff, removed_members_diff, removed_secret_members_diff,
    removed_space_members, secret_members, sort_members, typed_member, typed_members,
};
use crate::{ActorId, GroupId, MemberId, OperationId, SpaceEvent, SpaceId, StrongRemoveResolver};

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
pub struct Space<S, F, C> {
    /// Reference to the manager.
    ///
    /// This allows us build an API where users can treat "space" instances independently from the
    /// manager API, even though internally it has a reference to it.
    manager: Manager<S, F, C>,

    /// Id of the space.
    ///
    /// This is the "pointer" at the related space state which lives inside the manager.
    id: SpaceId,
}

impl<S, F, C> Space<S, F, C>
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
    pub(crate) fn new(manager_ref: Manager<S, F, C>, id: SpaceId) -> Self {
        Self {
            manager: manager_ref,
            id,
        }
    }

    /// Read the (dirty) state of this space within the given transaction.
    async fn state_tx(
        &self,
        tx: &StateTransaction<'_, S, F, C>,
    ) -> Result<SpaceState<C>, SpaceError<F, C>> {
        tx.space_state(&self.id)
            .await?
            .ok_or(SpaceError::UnknownSpace(self.id))
    }

    /// Get the latest committed space state.
    pub(crate) async fn state(&self) -> Result<SpaceState<C>, SpaceError<F, C>> {
        self.manager
            .read()
            .space_state(&self.id)
            .await?
            .ok_or(SpaceError::UnknownSpace(self.id))
    }

    /// Create a space containing initial members and access levels.
    ///
    /// If not already included, then the local actor (creator of this space) will be added to the
    /// initial members and given manage access level.
    ///
    /// Returns resulting global groups and space state and messages for processing.
    #[allow(clippy::type_complexity)]
    pub(crate) async fn create(
        ctx: &Context<'_, S, F>,
        global_groups_y: AuthGroupState<C>,
        key_manager_y: KeyManagerState,
        key_registry_y: KeyRegistryState<MemberId>,
        space_id: SpaceId,
        mut initial_members: Vec<(ActorId, Access<C>)>,
    ) -> Result<(AuthGroupState<C>, SpaceState<C>, SpaceOutput<C, F::Message>), SpaceError<F, C>>
    {
        let my_id = ctx.my_id();

        // Automatically add ourselves with "manage" level without any conditions as default.
        if !initial_members.iter().any(|(member, _)| *member == my_id) {
            initial_members.push((my_id, Access::manage()));
        }

        // Generate random group id.
        let group_id: VerifyingKey = {
            let signing_key = SigningKey::from_bytes(&ctx.rng.random_array()?);
            signing_key.verifying_key()
        };

        // Compute the groups to include in the space history based on initial group members.
        let include = typed_members(&global_groups_y, &initial_members)
            .iter()
            .filter_map(|(member, _)| match member {
                GroupMember::Individual(_) => None,
                GroupMember::Group(id) => Some(*id),
            })
            .collect::<Vec<_>>();

        // Instantiate new space state from existing global auth state.
        let space_y = SpaceState::init(my_id, space_id, group_id, key_manager_y, key_registry_y);
        let (space_y, from_group_messages) =
            Self::from_global_groups(ctx, &global_groups_y, space_y, &include).await?;

        let mut messages = vec![];
        messages.extend(
            from_group_messages
                .into_iter()
                .map(|message| (message, vec![])),
        );

        // Create the space group inside the global auth graph.
        let (global_groups_y, group_output) =
            Group::create(ctx, global_groups_y, group_id, initial_members).await?;
        let create_group_message = SpacesMessage::auth(&group_output.message);

        messages.push((group_output.message, vec![group_output.event]));

        // Apply the "create" auth message to the space state.
        let (space_y, message, events) =
            Self::process_auth_message(ctx, space_y, &create_group_message).await?;

        messages.push((message, events));

        Ok((global_groups_y, space_y, SpaceOutput { messages }))
    }

    /// Add a member to the space with assigned access level.
    ///
    /// Persists resulting state and returns messages for processing.
    pub async fn add(
        &self,
        member: impl Into<ActorId>,
        access: Access<C>,
    ) -> Result<SpaceOutput<C, F::Message>, SpaceError<F, C>> {
        let member = member.into();
        self.apply_and_commit(|y| AuthGroupAction::Add {
            member: typed_member(y, member),
            access,
        })
        .await
    }

    /// Remove a member from the space.
    ///
    /// Persists resulting state and returns messages for processing.
    pub async fn remove(
        &self,
        member: impl Into<ActorId>,
    ) -> Result<SpaceOutput<C, F::Message>, SpaceError<F, C>> {
        let member = member.into();
        self.apply_and_commit(|y| AuthGroupAction::Remove {
            member: typed_member(y, member),
        })
        .await
    }

    /// Promote an existing space member to the assigned access level.
    ///
    /// Persists resulting state and returns messages for processing.
    pub async fn promote(
        &self,
        member: impl Into<ActorId>,
        access: Access<C>,
    ) -> Result<SpaceOutput<C, F::Message>, SpaceError<F, C>> {
        let member = member.into();
        self.apply_and_commit(|y| AuthGroupAction::Promote {
            member: typed_member(y, member),
            access,
        })
        .await
    }

    /// Demote an existing space member to the assigned access level.
    ///
    /// Persists resulting state and returns messages for processing.
    pub async fn demote(
        &self,
        member: impl Into<ActorId>,
        access: Access<C>,
    ) -> Result<SpaceOutput<C, F::Message>, SpaceError<F, C>> {
        let member = member.into();
        self.apply_and_commit(|y| AuthGroupAction::Demote {
            member: typed_member(y, member),
            access,
        })
        .await
    }

    /// Apply an action on the space's group in the global groups state, apply the resulting auth
    /// message to the space itself and commit both states in one transaction.
    async fn apply_and_commit(
        &self,
        action: impl FnOnce(&AuthGroupState<C>) -> AuthGroupAction<C>,
    ) -> Result<SpaceOutput<C, F::Message>, SpaceError<F, C>> {
        let tx = self.manager.begin_write().await?;

        // Read.

        let global_groups_y = tx.global_groups_state().await?;
        let space_y = self.state_tx(&tx).await?;

        // Modify global group state.

        let action = action(&global_groups_y);
        let (global_groups_y, group_output) =
            Group::apply(&tx.ctx, global_groups_y, space_y.group_id, action).await?;

        // Modify local space state.

        let (space_y, space_message, space_events) = Self::process_auth_message(
            &tx.ctx,
            space_y,
            &SpacesMessage::auth(&group_output.message),
        )
        .await?;

        // Write.

        tx.commit(StateChanges::Space {
            global_groups_y: Some(global_groups_y),
            space_y,
        })
        .await?;

        let output = SpaceOutput::from(group_output, vec![(space_message, space_events)]);
        self.manager.waiters().notify_events(output.events());

        Ok(output)
    }

    /// Forge a "pointer" space message from an already existing auth message and apply any
    /// resulting group membership changes. Any resulting encryption direct messages are included
    /// in the space message alongside a reference to the auth message.
    async fn process_auth_message(
        ctx: &Context<'_, S, F>,
        mut space_y: SpaceState<C>,
        auth_message: &AuthMessage<C>,
    ) -> Result<(SpaceState<C>, F::Message, Vec<Event<C>>), SpaceError<F, C>> {
        // Get current space members.
        let current_members = space_y.groups_y.members(space_y.group_id);
        let current_secret_members = secret_members(&current_members);

        // Process auth message on local auth state.
        space_y.groups_y = AuthGroup::<C>::process(space_y.groups_y, auth_message)?;

        // Get next space members.
        let next_members = space_y.groups_y.members(space_y.group_id);
        let current_ancestors = space_y.groups_y.inner.ancestors(space_y.group_id);
        let next_secret_members = secret_members(&next_members);

        // Process the change of membership on encryption the context.
        let (encryption_y, direct_messages) = if current_secret_members != next_secret_members {
            Self::apply_secret_member_change(
                space_y.encryption_y,
                auth_message,
                &current_secret_members,
                &next_secret_members,
                ctx.rng,
            )?
        } else {
            (space_y.encryption_y, vec![])
        };

        space_y.encryption_y = encryption_y;

        // Construct space message and sign it in the forge (F)
        let dependencies: Vec<OperationId> = space_y.encryption_y.orderer.heads().to_vec();
        let space_message = {
            let args = SpacesArgs::SpaceMembership {
                space_id: space_y.space_id,
                group_id: space_y.group_id,
                space_dependencies: dependencies.clone(),
                auth_message_id: auth_message.id(),
                direct_messages,
            };

            // Forge and persist the message.
            ctx.forge(args).await?
        };

        // Update encryption dependencies.
        space_y
            .encryption_y
            .orderer
            .add_dependency(space_message.hash(), &dependencies);

        // Generate space events.
        let events = Self::generate_space_events(
            ctx.my_id(),
            &space_y,
            auth_message,
            &SpacesMessage::space_membership(&space_message),
            &current_members,
            &current_ancestors,
        );

        // Record any individuals that were removed from the space.
        update_removed(&mut space_y.removed, &current_members, &events);

        Ok((space_y, space_message, events))
    }

    /// Handle messages which effect the space membership. Each of these messages contained a
    /// pointer to an auth message and the auth message is required here.
    ///
    /// This method can be a no-op if all state transitions already happened previously
    /// (idempotency), in this case the returned state will be `None`.
    #[allow(clippy::type_complexity)]
    pub(crate) fn handle_membership_message(
        ctx: &Context<'_, S, F>,
        mut space_y: SpaceState<C>,
        space_message: &SpaceMembershipMessage,
        auth_message: &AuthMessage<C>,
    ) -> Result<(Option<SpaceState<C>>, Vec<Event<C>>), SpaceError<F, C>> {
        let SpaceMembershipMessage {
            id,
            space_dependencies,
            direct_messages,
            ..
        } = space_message;

        let me = ctx.my_id();
        let space_id = space_y.space_id;

        let current_members = space_y.groups_y.members(space_y.group_id);
        let current_ancestors = space_y.groups_y.inner.ancestors(space_y.group_id);
        let current_secret_members = secret_members(&current_members);

        // Process auth message on space auth state.
        //
        // Skip processing if this auth message has already been processed. This can happen when
        // multiple peers concurrently publish pointers to some auth message into the space.
        let duplicate_pointer = space_y
            .groups_y
            .inner
            .operations
            .contains_key(&auth_message.id());
        if !duplicate_pointer {
            space_y.groups_y = AuthGroup::<C>::process(space_y.groups_y, auth_message)?;
        } else {
            debug!(
                space_id = space_id.fmt_short(),
                groups_message = auth_message.id().fmt_short(),
                space_message = space_message.id.fmt_short(),
                "ignoring group change already applied to space"
            );
        };

        let mut application_events = vec![];

        // Process key agreement message.
        //
        // Internally this involves cryptography which can not be "repeated" (due to dropped
        // secrets, their forward-secret nature). Processing can thus never be done more than once.
        let has_seen = space_y.encryption_y.orderer.has_seen(*id);
        if has_seen {
            debug!(
                space_id = space_id.fmt_short(),
                space_message = space_message.id.fmt_short(),
                "ignore already processed space membership message"
            );

            debug_assert!(
                duplicate_pointer,
                "we never change group state without spaces state"
            );
        } else {
            let next_members = space_y.groups_y.members(space_y.group_id);
            let next_secret_members = secret_members(&next_members);
            let has_my_direct_message = direct_messages
                .iter()
                .any(|message| message.recipient == me);

            // Make the dgm aware of the new space members.
            space_y.encryption_y.dcgka.dgm.members = next_secret_members.clone();

            // If there are direct messages for us we need to process them.
            if has_my_direct_message {
                // Construct encryption message.
                let encryption_message = EncryptionMessage::from_membership(
                    space_message,
                    me,
                    auth_message,
                    &current_secret_members,
                    &next_secret_members,
                );

                // Process encryption message.
                let (encryption_y, encryption_output) =
                    EncryptionGroup::receive(space_y.encryption_y, &encryption_message)?;

                space_y.encryption_y = encryption_y;
                let events = encryption_output_to_space_events(&space_id, encryption_output);
                application_events.extend(events);
            };

            space_y
                .encryption_y
                .orderer
                .add_dependency(*id, space_dependencies);
        }

        // Even if we didn't change any state, we always want to return system events.
        let mut events = Self::generate_space_events(
            me,
            &space_y,
            auth_message,
            space_message,
            &current_members,
            &current_ancestors,
        );
        events.extend(application_events);

        if has_seen {
            return Ok((None, events));
        }

        // Record any individuals that were removed from the space.
        update_removed(&mut space_y.removed, &current_members, &events);

        Ok((Some(space_y), events))
    }

    /// Generate spaces events based on previous and next space members.
    fn generate_space_events(
        me: VerifyingKey,
        y: &SpaceState<C>,
        auth_message: &AuthMessage<C>,
        space_message: &SpaceMembershipMessage,
        previous_members: &[(ActorId, Access<C>)],
        previous_ancestors: &[ActorId],
    ) -> Vec<Event<C>> {
        let mut events = vec![];
        let next_members = y.groups_y.members(y.group_id);

        // Check if this membership change removes the local actor.
        let was_member = previous_members.iter().any(|(member, _)| member == &me);
        let is_member = next_members.iter().any(|(member, _)| member == &me);
        let ejected = was_member && !is_member;

        // Construct space membership event.
        let space_event = to_space_event(
            y.space_id,
            y.group_id,
            &y.groups_y,
            space_message,
            auth_message,
            previous_members,
            previous_ancestors,
        );

        events.push(space_event);

        if ejected {
            events.push(Event::Spaces(SpaceEvent::Ejected {
                space_id: y.space_id,
            }))
        }

        events
    }

    /// Apply a group membership change to the group encryption state.
    ///
    /// For "add" and "remove" actions, the difference between the current and next secret group
    /// members (those with "read" access) is computed and only the diff processed on the encryption
    /// group.
    #[allow(clippy::result_large_err)]
    fn apply_secret_member_change(
        mut encryption_y: EncryptionGroupState,
        auth_message: &AuthMessage<C>,
        current_members: &[ActorId],
        next_members: &[ActorId],
        rng: &Rng,
    ) -> Result<(EncryptionGroupState, Vec<EncryptionDirectMessage>), SpaceError<F, C>> {
        // Make the DGM aware of group members after this group membership change has been
        // processed.
        encryption_y.dcgka.dgm = EncryptionMembershipState {
            members: next_members.to_vec(),
        };

        let mut direct_messages = vec![];
        let encryption_y = {
            match &auth_message.action() {
                AuthGroupAction::Create { .. } => {
                    let (encryption_y, message) =
                        EncryptionGroup::create(encryption_y, next_members.to_vec(), rng)?;
                    direct_messages.extend(message.direct_messages());
                    encryption_y
                }
                AuthGroupAction::Add { .. } | AuthGroupAction::Promote { .. } => {
                    let all_added = added_secret_members_diff(current_members, next_members);

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
                AuthGroupAction::Remove { .. } | AuthGroupAction::Demote { .. } => {
                    let all_removed = removed_secret_members_diff(current_members, next_members);

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
            }
        };

        Ok((encryption_y, direct_messages))
    }

    /// Handle space application messages.
    #[allow(clippy::type_complexity)]
    pub(crate) fn handle_application_message(
        mut space_y: SpaceState<C>,
        message: &ApplicationMessage,
    ) -> Result<(SpaceState<C>, Vec<Event<C>>), SpaceError<F, C>> {
        // Check that the publisher of this application message has write access.
        verify_claimed_write_access::<_, _, _, _, StrongRemoveResolver<C>>(
            &space_y.groups_y,
            message.author,
            space_y.group_id,
            HashSet::from_iter(message.proof.clone()),
        )?;

        // Process encryption message.
        let encryption_message = EncryptionMessage::from_application(message);
        let (encryption_y, encryption_output) =
            EncryptionGroup::receive(space_y.encryption_y, &encryption_message)?;

        space_y.encryption_y = encryption_y;

        // Update dependencies.
        space_y
            .encryption_y
            .orderer
            .add_dependency(encryption_message.id(), &message.space_dependencies);

        let mut events = encryption_output_to_space_events(&space_y.space_id, encryption_output);

        // Filter out application events if the author has concurrently lost their write access.
        //
        // This case occurs if a member published messages before receiving the control message
        // which removed them. Their claim to have write access is still historically valid,
        // however we have already learned that they have actually been removed.
        //
        // NOTE: Application _and_ control messages are partially-ordered based on their causal
        // relationships, application messages are always emitted together with the member's
        // (create/add) welcome message. For that reason application messages from members which
        // have been removed at a causally later point (not concurrently) will not be affected by
        // this filter and are correctly forwarded to the application layer.
        let has_write_access =
            space_y
                .groups_y
                .members(space_y.group_id)
                .iter()
                .any(|(member, access)| {
                    *member == message.author && (access.is_write() || access.is_manage())
                });

        if !has_write_access {
            events.retain(|event| !matches!(event, Event::Application { .. }));
        }

        Ok((space_y, events))
    }

    /// Incorporate any missing groups messages into the space state.
    ///
    /// Forges messages within the currently open transaction and returns the resulting space
    /// state, without persisting it. The returned state is `None` if nothing needed to be
    /// repaired.
    ///
    /// Returns all existing group messages which needed incorporating into the space as well as
    /// any newly forged space membership messages.
    pub(crate) async fn apply_repair(
        ctx: &Context<'_, S, F>,
        global_groups_y: &AuthGroupState<C>,
        mut space_y: SpaceState<C>,
        groups: &[GroupId],
    ) -> Result<(Option<SpaceState<C>>, RepairOutput<C, F::Message>), SpaceError<F, C>> {
        // TODO: we can optimize here by calculating the diff between the current space auth graph
        // tips and the global auth graph tips. Then we could apply only the diff, rather than
        // processing all operations as we do here.
        let mut messages = vec![];
        for id in global_groups_y.inner.toposort(groups) {
            // This auth message has already been processed by the space.
            if space_y.groups_y.inner.operations.contains_key(&id) {
                continue;
            };

            let group_message = ctx
                .store
                .get_spaces_message_tx(&id)
                .await
                .map_err(|err| StoreError::MessageStore(err.to_string()))?
                .expect("all auth messages exist");

            let (space_y_i, space_message, space_events) =
                Self::process_auth_message(ctx, space_y, &SpacesMessage::auth(&group_message))
                    .await?;

            space_y = space_y_i;
            messages.push((group_message, vec![]));
            messages.push((space_message, space_events));
        }

        if messages.is_empty() {
            return Ok((None, RepairOutput::from(messages)));
        }

        Ok((Some(space_y), RepairOutput::from(messages)))
    }

    /// Incorporate any missing groups messages into the space state.
    ///
    /// Persists resulting state and returns forged messages.
    pub async fn repair(
        &self,
        groups: &[GroupId],
    ) -> Result<RepairOutput<C, F::Message>, SpaceError<F, C>> {
        let tx = self.manager.begin_write().await?;

        // Read.

        let global_groups_y = tx.global_groups_state().await?;
        let space_y = self.state_tx(&tx).await?;

        // Modify.

        let (space_y, output) =
            Self::apply_repair(&tx.ctx, &global_groups_y, space_y, groups).await?;

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

        self.manager.waiters().notify_events(output.events());

        Ok(output)
    }

    /// Instantiate space state from existing global auth state.
    ///
    /// Every space contains pointers to all messages published to the global auth state. This
    /// method iterates through all existing auth messages and publishes the required pointers to
    /// the space.
    ///
    /// None of the messages will contain encryption control messages as they were published before
    /// the space existed.
    async fn from_global_groups(
        ctx: &Context<'_, S, F>,
        global_groups_y: &AuthGroupState<C>,
        mut space_y: SpaceState<C>,
        include: &[GroupId],
    ) -> Result<(SpaceState<C>, Vec<F::Message>), SpaceError<F, C>> {
        let mut messages = vec![];

        // Publish pointers for all operations in the global auth graph. We topologically sort the
        // operations and publish them in this linear order.
        //
        // These won't contain any encryption messages as they were published _before_ the space was
        // created.
        //
        // TODO: Instead of pointing at each message and creating a `SpaceMembership` message for
        // _each_ of them (this can become a lot of messages), would it be possible to only point
        // one single message (ideally the CREATE space message) at the required tips?
        let mut space_dependencies = vec![];
        for id in global_groups_y.inner.toposort(include) {
            let group_message = ctx
                .store
                .get_spaces_message_tx(&id)
                .await
                .map_err(|err| StoreError::MessageStore(err.to_string()))?
                .expect("all auth operations exist");

            // Apply the group message from the global state onto the local space state.
            space_y.groups_y =
                AuthGroup::<C>::process(space_y.groups_y, &SpacesMessage::auth(&group_message))?;

            let args = SpacesArgs::SpaceMembership {
                space_id: space_y.space_id,
                group_id: space_y.group_id,
                auth_message_id: group_message.hash(),
                direct_messages: vec![],
                space_dependencies: space_dependencies.clone(),
            };
            let space_message = ctx.forge(args).await?;

            space_y
                .encryption_y
                .orderer
                .add_dependency(space_message.hash(), &space_dependencies);

            space_dependencies = vec![space_message.hash()];

            messages.push(group_message);
            messages.push(space_message);
        }

        Ok((space_y, messages))
    }

    /// Verifying key of the local actor.
    pub fn me(&self) -> VerifyingKey {
        self.manager.id()
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
        let mut members = y.groups_y.members(y.group_id);
        sort_members(&mut members);
        Ok(members)
    }

    /// Returns all members who were removed from, and not re-admitted to, this space.
    pub async fn removed(&self) -> Result<Vec<MemberId>, SpaceError<F, C>> {
        Ok(removed_space_members(&self.state().await?))
    }

    /// All actors (both groups and individuals) in the space.
    pub async fn actors(&self) -> Result<Vec<(MemberId, Access<C>)>, SpaceError<F, C>> {
        let y = self.state().await?;
        let mut members: Vec<(VerifyingKey, Access<C>)> = y
            .groups_y
            .root_members(y.group_id)
            .into_iter()
            .map(|(member, access)| (member.id(), access))
            .collect();
        sort_members(&mut members);
        Ok(members)
    }

    /// Publish a message encrypted towards all current group members.
    ///
    /// Persists resulting state and returns forged message.
    pub async fn publish(
        &self,
        plaintext: &[u8],
    ) -> Result<(F::Message, Event<C>), SpaceError<F, C>> {
        let tx = self.manager.begin_write().await?;

        // Read.

        let space_y = self.state_tx(&tx).await?;

        // Modify.

        let (space_y, message, event) = Self::apply_publish(&tx.ctx, space_y, plaintext).await?;

        // Write.

        tx.commit(StateChanges::Space {
            global_groups_y: None,
            space_y,
        })
        .await?;

        self.manager.waiters().notify_events([&event]);

        Ok((message, event))
    }

    /// Encrypt plaintext towards all current group members.
    ///
    /// Forges the message within the currently open transaction and returns the resulting space
    /// state, without persisting it.
    pub(crate) async fn apply_publish(
        ctx: &Context<'_, S, F>,
        mut space_y: SpaceState<C>,
        plaintext: &[u8],
    ) -> Result<(SpaceState<C>, F::Message, Event<C>), SpaceError<F, C>> {
        if !space_y.encryption_y.orderer.is_welcomed() {
            return Err(SpaceError::NotWelcomed(space_y.space_id));
        }

        // Encrypt plaintext towards encryption group members.
        let (encryption_y, encryption_args) =
            EncryptionGroup::send(space_y.encryption_y, plaintext, ctx.rng)?;
        space_y.encryption_y = encryption_y;

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
                space_id: space_y.space_id,
                space_dependencies: dependencies.clone(),
                proof: space_y.groups_y.heads(&[space_y.group_id]),
                group_secret_id,
                nonce,
                ciphertext,
            };
            (args, dependencies)
        };

        // Forge message.
        let message = ctx.forge(args).await?;

        // Update dependencies.
        space_y
            .encryption_y
            .orderer
            .add_dependency(message.hash(), &dependencies);

        let event = Event::Application {
            space_id: space_y.space_id,
            data: plaintext.to_owned(),
        };

        Ok((space_y, message, event))
    }
}

/// Output from methods which create a new space or change the membership of an existing space.
///
/// Messages need to be published and system events forwarded to the application layer. All
/// resulting state was already persisted.
#[derive(Debug)]
pub struct SpaceOutput<C, M> {
    /// Messages and their associated system events for "enriched" context.
    pub messages: Vec<(M, Vec<Event<C>>)>,
}

impl<C, M> SpaceOutput<C, M> {
    pub(crate) fn from(
        global_groups_output: GroupOutput<C, M>,
        mut messages: Vec<(M, Vec<Event<C>>)>,
    ) -> Self {
        // We insert the group messages and event to the front so that they come before space
        // ones.
        messages.insert(
            0,
            (
                global_groups_output.message,
                vec![global_groups_output.event],
            ),
        );
        Self { messages }
    }

    pub fn events(&self) -> impl Iterator<Item = &Event<C>> {
        self.messages.iter().flat_map(|(_, events)| events)
    }
}

/// Output from methods which repair a space.
#[derive(Debug)]
pub struct RepairOutput<C, M> {
    pub messages: Vec<(M, Vec<Event<C>>)>,
}

impl<C, M> RepairOutput<C, M> {
    pub fn from(messages: Vec<(M, Vec<Event<C>>)>) -> Self {
        Self { messages }
    }

    pub fn events(&self) -> impl Iterator<Item = &Event<C>> {
        self.messages.iter().flat_map(|(_, events)| events)
    }
}

fn update_removed<C: Conditions>(
    removed: &mut HashSet<MemberId>,
    current_members: &[(MemberId, Access<C>)],
    events: &[Event<C>],
) {
    for event in events.iter() {
        if let Event::Spaces(SpaceEvent::Removed { context, .. }) = &event {
            // We accumulate all members who were ever removed from a space here. If only the
            // diff was used, it would be harder for consumers to implement block patterns
            // based on this field as information about historic removals would be lost. To
            // account for re-adds the current membership of the group can be compared against
            // the remove list.
            removed.extend(
                removed_members_diff(current_members, &context.members)
                    .into_iter()
                    .map(|(member, _)| member),
            );
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

    #[error(transparent)]
    State(#[from] StateError),

    #[error("tried to access unknown space id {0}")]
    UnknownSpace(SpaceId),

    #[error("tried to publish when not a member of space {0}")]
    NotWelcomed(SpaceId),

    #[error(transparent)]
    UnauthorizedWrite(#[from] VerifyClaimedWriteError),
}
