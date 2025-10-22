// SPDX-License-Identifier: MIT OR Apache-2.0

//! High-level API for managing spaces, groups and member keys.
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use p2panda_auth::Access;
use p2panda_auth::traits::{Conditions, Operation};
use p2panda_encryption::Rng;
use petgraph::algo::toposort;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::auth::message::AuthMessage;
use crate::event::Event;
use crate::group::{Group, GroupError};
use crate::identity::{IdentityError, IdentityManager};
use crate::member::Member;
use crate::message::SpacesArgs;
use crate::space::{Space, SpaceError};
use crate::traits::{
    AuthStore, AuthoredMessage, Forge, KeyRegistryStore, KeySecretStore, MessageStore, SpaceId,
    SpacesMessage, SpacesStore,
};
use crate::types::{ActorId, AuthResolver, OperationId};
use crate::{Config, Credentials};

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
pub struct Manager<ID, S, K, F, M, C, RS> {
    pub(crate) actor_id: ActorId,
    #[allow(clippy::type_complexity)]
    pub(crate) inner: Arc<RwLock<ManagerInner<ID, S, K, F, M, C, RS>>>,
}

#[derive(Debug)]
pub(crate) struct ManagerInner<ID, S, K, F, M, C, RS> {
    pub(crate) store: S,
    pub(crate) identity: IdentityManager<ID, K, F, M, C>,
    pub(crate) rng: Rng,
    _marker: PhantomData<(F, RS)>,
}

impl<ID, S, K, F, M, C, RS> Manager<ID, S, K, F, M, C, RS>
where
    ID: SpaceId,
    // @TODO: the Debug bound is required as we are string formatting the manager error in
    // groups.rs due to challenges handling cyclical errors. If that issue is solved in a more
    // satisfactory way then this bound can be removed.
    S: SpacesStore<ID, M, C> + AuthStore<C> + MessageStore<M> + Debug,
    K: KeyRegistryStore + KeySecretStore + Debug,
    F: Forge<ID, M, C> + Debug,
    M: AuthoredMessage + SpacesMessage<ID, C> + Debug,
    C: Conditions,
    RS: AuthResolver<C> + Debug,
{
    /// Instantiate a new manager.
    #[allow(clippy::result_large_err)]
    pub async fn new(
        store: S,
        key_store: K,
        forge: F,
        credentials: Credentials,
        rng: Rng,
    ) -> Result<Self, ManagerError<ID, S, K, F, M, C, RS>> {
        Self::new_with_config(
            store,
            key_store,
            forge,
            credentials,
            &Config::default(),
            rng,
        )
        .await
    }

    /// Instantiate a new manager with custom configuration.
    #[allow(clippy::result_large_err)]
    pub async fn new_with_config(
        store: S,
        key_store: K,
        forge: F,
        credentials: Credentials,
        config: &Config,
        rng: Rng,
    ) -> Result<Self, ManagerError<ID, S, K, F, M, C, RS>> {
        let actor_id: ActorId = credentials.public_key().into();
        let identity = IdentityManager::new(key_store, forge, credentials, config, &rng).await?;
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
        id: ID,
    ) -> Result<Option<Space<ID, S, K, F, M, C, RS>>, ManagerError<ID, S, K, F, M, C, RS>> {
        let has_space = {
            let inner = self.inner.read().await;
            inner
                .store
                .has_space(&id)
                .await
                .map_err(ManagerError::SpacesStore)?
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
        id: ActorId,
    ) -> Result<Option<Group<ID, S, K, F, M, C, RS>>, ManagerError<ID, S, K, F, M, C, RS>> {
        let auth_y = {
            let manager = self.inner.read().await;
            manager.store.auth().await.map_err(GroupError::AuthStore)?
        };

        // Check if this group exists in the auth state.
        if auth_y.has_group(id) {
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
    /// Returns messages for replication to other instances and events which inform users of any
    /// state changes which occurred.
    pub async fn create_space(
        &self,
        id: ID,
        initial_members: &[(ActorId, Access<C>)],
    ) -> Result<
        (Space<ID, S, K, F, M, C, RS>, Vec<M>, Vec<Event<ID, C>>),
        ManagerError<ID, S, K, F, M, C, RS>,
    > {
        let (space, messages, events) = Space::create(self.clone(), id, initial_members.to_owned())
            .await
            .map_err(ManagerError::Space)?;

        Ok((space, messages, events))
    }

    /// Create a new group containing initial members with associated access levels.
    ///
    /// It is possible to create a group where the creator is not an initial member or is a member
    /// without manager rights. If this is done then after creation no further change of the group
    /// membership would be possible.
    ///
    /// Returns messages for replication to other instances and events which inform users of any
    /// state changes which occurred.
    pub async fn create_group(
        &self,
        initial_members: &[(ActorId, Access<C>)],
    ) -> Result<
        (Group<ID, S, K, F, M, C, RS>, Vec<M>, Event<ID, C>),
        ManagerError<ID, S, K, F, M, C, RS>,
    > {
        let (group, messages, event) = Group::create(self.clone(), initial_members.to_owned())
            .await
            .map_err(ManagerError::Group)?;

        Ok((group, messages, event))
    }

    /// Process a spaces message.
    ///
    /// We expect messages to be signature-checked, dependency-checked & partially ordered.
    ///
    /// Returns events which inform users of any state changes which occurred.
    pub async fn process(
        &self,
        message: &M,
    ) -> Result<Vec<Event<ID, C>>, ManagerError<ID, S, K, F, M, C, RS>> {
        // Route message to the regarding member-, group- or space processor.
        let events = match message.args() {
            // Received key bundle from a member.
            SpacesArgs::KeyBundle { key_bundle } => {
                let mut manager = self.inner.write().await;
                let event = manager
                    .identity
                    .process_key_bundle(message.author(), key_bundle)
                    .await
                    .map_err(ManagerError::IdentityManager)?;

                vec![event]
            }
            SpacesArgs::Auth { .. } => {
                let event = Group::process(self.clone(), message)
                    .await
                    .map_err(ManagerError::Group)?;

                if let Some(event) = event {
                    vec![event]
                } else {
                    vec![]
                }
            }
            // Received control message related to a space.
            SpacesArgs::SpaceMembership { .. } => {
                self.handle_space_membership_message(message).await?
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

    /// The public key of the local actor.
    pub fn id(&self) -> ActorId {
        self.actor_id
    }

    /// The local actor id and their long-term key bundle.
    ///
    /// Note: Key bundle will be rotated if the latest is reaching it's configured expiry date.
    pub async fn me(&self) -> Result<Member, ManagerError<ID, S, K, F, M, C, RS>> {
        let mut manager = self.inner.write().await;
        manager
            .identity
            .me()
            .await
            .map_err(ManagerError::IdentityManager)
    }

    /// Register a member with long-term key bundle material which was provided through another
    /// channel (QR code scan etc.).
    pub async fn register_member(
        &self,
        member: &Member,
    ) -> Result<(), ManagerError<ID, S, K, F, M, C, RS>> {
        let mut manager = self.inner.write().await;
        manager
            .identity
            .register_member(member)
            .await
            .map_err(ManagerError::IdentityManager)
    }

    /// Check if my latest key bundle has expired.
    ///
    /// If `true` then users should rotate their pre-key and generate a new bundle message (which
    /// should then be published) by calling `key_bundle_message`.
    pub async fn key_bundle_expired(&self) -> Result<bool, ManagerError<ID, S, K, F, M, C, RS>> {
        let manager = self.inner.read().await;
        Ok(manager.identity.key_bundle_expired().await?)
    }

    /// Forge a key bundle message containing my latest key bundle.
    ///
    /// Note: Key bundle will be rotated if the latest is reaching it's configured expiry date.
    pub async fn key_bundle_message(&self) -> Result<M, ManagerError<ID, S, K, F, M, C, RS>> {
        let mut manager = self.inner.write().await;
        manager
            .identity
            .key_bundle_message()
            .await
            .map_err(ManagerError::IdentityManager)
    }

    /// Returns a list of all spaces which are "out-of-sync" with the global shared auth state.
    pub async fn spaces_repair_required(
        &self,
    ) -> Result<Vec<ID>, ManagerError<ID, S, K, F, M, C, RS>> {
        let manager = self.inner.read().await;

        let auth_y = manager
            .store
            .auth()
            .await
            .map_err(ManagerError::AuthStore)?;

        let space_ids = manager
            .store
            .spaces_ids()
            .await
            .map_err(ManagerError::SpacesStore)?;

        let mut in_need_of_repair = vec![];
        for id in space_ids {
            let space_y = manager
                .store
                .space(&id)
                .await
                .map_err(ManagerError::SpacesStore)?
                .expect("space present in store");
            if space_y.auth_y.inner.heads() != auth_y.inner.heads() {
                in_need_of_repair.push(id);
            }
        }

        Ok(in_need_of_repair)
    }

    /// Publish a reference to any auth messages missing from the passed spaces.
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
    /// On identifying that a space needs "repairing" by calling spaces_repair_required(), _any_
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
    pub async fn repair_spaces(
        &self,
        space_ids: &Vec<ID>,
    ) -> Result<(Vec<M>, Vec<Event<ID, C>>), ManagerError<ID, S, K, F, M, C, RS>> {
        let auth_y = {
            let manager = self.inner.read().await;
            manager
                .store
                .auth()
                .await
                .map_err(ManagerError::AuthStore)?
        };
        let operation_ids =
            toposort(&auth_y.inner.graph, None).expect("auth graph does not contain cycles");

        let mut messages = vec![];
        let mut events = vec![];
        // @TODO: we can optimize here by calculating the diff between the current space auth
        // graph tips and the global auth graph tips. Then we could apply only the missing
        // operations rather than applying all operations as we do here.
        for id in operation_ids {
            let message = {
                let manager = self.inner.read().await;
                manager
                    .store
                    .message(&id)
                    .await
                    .map_err(ManagerError::MessageStore)?
                    .expect("message present in store")
            };
            for id in space_ids {
                let (message, event) = self.apply_group_change_to_space(&message, *id).await?;
                if let Some(message) = message {
                    messages.push(message);
                }
                if let Some(event) = event {
                    events.push(event)
                }
            }
        }

        Ok((messages, events))
    }

    /// Apply an auth message from the shared auth state to each space we know about locally.
    ///
    /// This is required so that all spaces stay "in sync" with the shared auth state and produce
    /// any required encryption direct messages in order to correctly update a spaces' encryption
    /// state.
    pub(crate) async fn apply_group_change_to_spaces(
        &self,
        auth_message: &M,
    ) -> Result<(Vec<M>, Vec<Event<ID, C>>), ManagerError<ID, S, K, F, M, C, RS>> {
        let space_ids = {
            let manager = self.inner.read().await;
            manager
                .store
                .spaces_ids()
                .await
                .map_err(ManagerError::SpacesStore)?
        };

        let mut messages = vec![];
        let mut events = vec![];
        for id in space_ids {
            let (message, event) = self.apply_group_change_to_space(auth_message, id).await?;
            if let Some(message) = message {
                messages.push(message);
            }
            if let Some(event) = event {
                events.push(event)
            }
        }

        Ok((messages, events))
    }

    /// Apply a message from the shared auth state to a single space.
    pub(crate) async fn apply_group_change_to_space(
        &self,
        auth_message: &M,
        space_id: ID,
    ) -> Result<(Option<M>, Option<Event<ID, C>>), ManagerError<ID, S, K, F, M, C, RS>> {
        let Some(space) = self.space(space_id).await? else {
            panic!("expect space to exist");
        };
        space
            .handle_auth_group_change(auth_message)
            .await
            .map_err(ManagerError::Space)
    }

    async fn handle_space_membership_message(
        &self,
        message: &M,
    ) -> Result<Vec<Event<ID, C>>, ManagerError<ID, S, K, F, M, C, RS>> {
        let SpacesArgs::SpaceMembership {
            space_id,
            auth_message_id,
            ..
        } = message.args()
        else {
            panic!("unexpected message type");
        };

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
            .map_err(ManagerError::Space)
    }

    /// Persist a message in the message store.
    ///
    /// Only exposed for testing purposes as in normal use we expect all messages to be already
    /// persisted in the store.
    #[cfg(test)]
    pub async fn persist_message(
        &self,
        message: &M,
    ) -> Result<(), ManagerError<ID, S, K, F, M, C, RS>> {
        let manager = self.inner.write().await;
        manager
            .store
            .set_message(&message.id(), message)
            .await
            .map_err(ManagerError::MessageStore)?;
        Ok(())
    }
}

// Deriving clone on Manager will enforce generics to also impl Clone even though we are wrapping
// them in an Arc. Related: https://stackoverflow.com/questions/72150623
impl<ID, S, K, F, M, C, RS> Clone for Manager<ID, S, K, F, M, C, RS> {
    fn clone(&self) -> Self {
        Self {
            actor_id: self.actor_id,
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum ManagerError<ID, S, K, F, M, C, RS>
where
    ID: SpaceId,
    S: SpacesStore<ID, M, C> + AuthStore<C> + MessageStore<M>,
    K: KeyRegistryStore + KeySecretStore + Debug,
    F: Forge<ID, M, C> + Debug,
    C: Conditions,
    RS: AuthResolver<C> + Debug,
{
    #[error(transparent)]
    Space(#[from] SpaceError<ID, S, K, F, M, C, RS>),

    #[error(transparent)]
    Group(#[from] GroupError<ID, S, K, F, M, C, RS>),

    #[error(transparent)]
    IdentityManager(#[from] IdentityError<ID, K, F, M, C>),

    #[error("{0}")]
    SpacesStore(<S as SpacesStore<ID, M, C>>::Error),

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
