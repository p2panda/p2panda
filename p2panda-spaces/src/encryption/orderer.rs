// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::VecDeque;
use std::fmt::Debug;
use std::marker::PhantomData;

use p2panda_auth::traits::Conditions;
use p2panda_encryption::crypto::xchacha20::XAeadNonce;
use p2panda_encryption::data_scheme::GroupSecretId;
use p2panda_encryption::traits::GroupMessage;
use petgraph::prelude::DiGraphMap;
use petgraph::visit::NodeIndexable;
use thiserror::Error;
use tokio::runtime::Handle;

use crate::encryption::dgm::EncryptionGroupMembership;
use crate::encryption::message::{EncryptionArgs, EncryptionMessage};
use crate::traits::{AuthoredMessage, MessageStore, SpaceId, SpacesMessage};
use crate::types::{ActorId, EncryptionControlMessage, EncryptionDirectMessage, OperationId};

/// Implementation of Ordering trait from p2panda-encryption which computes
/// dependencies for encryption messages and performs some internal buffering. It does _not_ take
/// care of ordering of control and application messages; p2panda-spaces expects messages to be
/// orderer before being processed.
#[derive(Clone, Default, Debug)]
pub struct EncryptionOrderer<ID, S, M, C> {
    _marker: PhantomData<(ID, S, M, C)>,
}

impl<ID, S, M, C> EncryptionOrderer<ID, S, M, C>
where
    S: MessageStore<M>,
{
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

/// Orderer for encryption messages.
#[derive(Clone, Debug)]
pub struct EncryptionOrdererState<ID, S, M, C> {
    /// Current graph heads (cache)
    heads: Vec<OperationId>,

    // @TODO: currently application messages are also included in the dependency graph, we want
    // to separate these from control messages eventually in order to support pruning.
    /// Graph of all operations processed by this group.
    graph: DiGraphMap<OperationId, ()>,

    /// Queue of operations we have not yet processed.
    ///
    /// This should only grow in the case where we are not yet welcomed into the group by an "Add"
    /// message.
    queue: VecDeque<OperationId>,

    /// "Create" or "Add" message which got us into the group.
    welcome_message: Option<EncryptionMessage>,

    /// Store with read-access to all messages.
    store: S,

    _marker: PhantomData<(ID, M, C)>,
}

impl<ID, S, M, C> EncryptionOrdererState<ID, S, M, C> {
    pub fn new(store: S) -> Self {
        Self {
            heads: Default::default(),
            // @TODO: We don't look at application message dependencies quite yet. More research
            // needed into requirements around bi-directional dependencies between DAGs.
            graph: Default::default(),
            queue: Default::default(),
            store,
            welcome_message: Default::default(),
            _marker: PhantomData,
        }
    }

    /// Add a new dependency relationship to the operation graph.
    pub fn add_dependency(&mut self, id: OperationId, dependencies: &[OperationId]) {
        if self.graph.contains_node(id) {
            return;
        }
        self.graph.add_node(id);
        for dependency in dependencies {
            self.graph.add_edge(*dependency, id, ());
        }

        self.heads = self
            .graph
            .clone()
            .into_graph::<usize>()
            .externals(petgraph::Direction::Outgoing)
            .map(|idx| self.graph.from_index(idx.index()))
            .collect::<Vec<_>>();
    }

    /// Get the current dependency graph heads.
    pub fn heads(&self) -> &[OperationId] {
        &self.heads
    }

    /// Has the local actor been welcomed to the encryption group.
    pub fn is_welcomed(&self) -> bool {
        self.welcome_message.is_some()
    }

    /// Has the orderer seen a certain message.
    pub fn has_seen(&self, id: OperationId) -> bool {
        self.graph.contains_node(id)
    }
}

impl<ID, S, M, C>
    p2panda_encryption::traits::Ordering<ActorId, OperationId, EncryptionGroupMembership>
    for EncryptionOrderer<ID, S, M, C>
where
    ID: SpaceId,
    M: SpacesMessage<ID, C> + AuthoredMessage + Debug,
    S: MessageStore<M> + Debug,
    C: Conditions,
{
    type State = EncryptionOrdererState<ID, S, M, C>;

    type Error = EncryptionOrdererError<S, M>;

    type Message = EncryptionMessage;

    fn next_control_message(
        y: Self::State,
        control_message: &EncryptionControlMessage,
        direct_messages: &[EncryptionDirectMessage],
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        let dependencies = y.heads().to_vec();
        Ok((
            y,
            EncryptionMessage::Args(EncryptionArgs::System {
                dependencies,
                control_message: control_message.clone(),
                direct_messages: direct_messages.to_vec(),
            }),
        ))
    }

    fn next_application_message(
        y: Self::State,
        group_secret_id: GroupSecretId,
        nonce: XAeadNonce,
        ciphertext: Vec<u8>,
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        let dependencies = y.heads().to_vec();
        Ok((
            y,
            EncryptionMessage::Args(EncryptionArgs::Application {
                dependencies,
                group_secret_id,
                nonce,
                ciphertext,
            }),
        ))
    }

    fn queue(mut y: Self::State, message: &Self::Message) -> Result<Self::State, Self::Error> {
        let id = message.id();
        y.queue.push_back(id);
        Ok(y)
    }

    fn set_welcome(
        mut y: Self::State,
        message: &Self::Message,
    ) -> Result<Self::State, Self::Error> {
        y.welcome_message = Some(message.clone());
        Ok(y)
    }

    fn next_ready_message(
        mut y: Self::State,
    ) -> Result<(Self::State, Option<Self::Message>), Self::Error> {
        // We have not joined the group yet, don't process any messages.
        if y.welcome_message.is_none() {
            return Ok((y, None));
        };

        let Some(id) = y.queue.pop_front() else {
            return Ok((y, None));
        };

        let (message, y) = tokio::task::block_in_place(|| {
            Handle::current().block_on(async move {
                let message = y
                    .store
                    .message(&id)
                    .await
                    .map_err(|err| EncryptionOrdererError::OperationStore(err))?
                    .ok_or(EncryptionOrdererError::StoreInconsistency(id))?;
                Ok((message, y))
            })
        })?;

        Ok((y, Some(EncryptionMessage::from_application(&message))))
    }
}

#[derive(Debug, Error)]
pub enum EncryptionOrdererError<S, M>
where
    S: MessageStore<M>,
{
    #[error("could not find item with id {0} in operation store")]
    StoreInconsistency(OperationId),

    #[error("{0}")]
    OperationStore(S::Error),
}
