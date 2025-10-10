// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::marker::PhantomData;

use p2panda_encryption::crypto::xchacha20::XAeadNonce;
use p2panda_encryption::data_scheme::GroupSecretId;
use p2panda_encryption::traits::GroupMessage;
use petgraph::prelude::DiGraphMap;
use petgraph::visit::NodeIndexable;

use crate::encryption::dgm::EncryptionGroupMembership;
use crate::encryption::message::{EncryptionArgs, EncryptionMessage};
use crate::types::{ActorId, EncryptionControlMessage, EncryptionDirectMessage, OperationId};

/// Implementation of Ordering trait from p2panda-encryption which computes
/// dependencies for encryption messages and performs some internal buffering. It does _not_ take
/// care of ordering of control and application messages; p2panda-spaces expects messages to be
/// orderer before being processed.
#[derive(Clone, Debug)]
pub struct EncryptionOrderer<M> {
    _marker: PhantomData<M>,
}

impl<M> Default for EncryptionOrderer<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M> EncryptionOrderer<M> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

/// Orderer for encryption messages.
#[derive(Clone, Debug)]
pub struct EncryptionOrdererState {
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

    // @TODO: We keep all messages in memory currently which is bad. We need a persistence
    // layer where we can fetch messages from.
    /// In-memory store of all messages.
    messages: HashMap<OperationId, EncryptionMessage>,

    /// "Create" or "Add" message which got us into the group.
    welcome_message: Option<EncryptionMessage>,
}

impl Default for EncryptionOrdererState {
    fn default() -> Self {
        Self::new()
    }
}

impl EncryptionOrdererState {
    /// Instantiate new orderer state.
    pub fn new() -> Self {
        Self {
            heads: Default::default(),
            graph: Default::default(),
            queue: Default::default(),
            messages: Default::default(),
            welcome_message: Default::default(),
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

impl<M> p2panda_encryption::traits::Ordering<ActorId, OperationId, EncryptionGroupMembership>
    for EncryptionOrderer<M>
{
    type State = EncryptionOrdererState;

    type Error = Infallible;

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
        y.messages.insert(id, message.clone());
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

        let message = y.queue.pop_front().map(|id| {
            y.messages
                .get(&id)
                .expect("ids map consistently to messages")
                .to_owned()
        });

        Ok((y, message))
    }
}
