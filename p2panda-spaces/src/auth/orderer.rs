// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;

use p2panda_auth::traits::Conditions;
use petgraph::prelude::DiGraphMap;
use petgraph::visit::NodeIndexable;

use crate::auth::message::{AuthArgs, AuthMessage};
use crate::types::{ActorId, AuthControlMessage, OperationId};

/// Implementation of Orderer trait from p2panda-auth which computes dependencies for auth
/// messages. It does _not_ take care of ordering of control and application messages,
/// p2panda-spaces expects messages to be orderer before being processed.
#[derive(Clone, Debug)]
pub struct AuthOrdererState {
    pub heads: Vec<OperationId>,
    pub graph: DiGraphMap<OperationId, ()>,
}

impl Default for AuthOrdererState {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthOrdererState {
    pub fn new() -> Self {
        Self {
            heads: Default::default(),
            graph: Default::default(),
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
}

// Manages "dependencies" required for `p2panda-auth`.
#[derive(Clone, Debug)]
pub struct AuthOrderer {}

impl AuthOrderer {
    pub fn init() -> AuthOrdererState {
        AuthOrdererState::new()
    }
}

impl<C> p2panda_auth::traits::Orderer<ActorId, OperationId, AuthControlMessage<C>> for AuthOrderer
where
    C: Conditions,
{
    type State = AuthOrdererState;

    type Operation = AuthMessage<C>;

    type Error = Infallible; // @TODO

    fn next_message(
        y: Self::State,
        control_message: &AuthControlMessage<C>,
    ) -> Result<(Self::State, Self::Operation), Self::Error> {
        let dependencies = y.heads().to_vec();
        Ok((
            y,
            AuthMessage::Args(AuthArgs {
                dependencies,
                control_message: control_message.clone(),
            }),
        ))
    }

    fn queue(_y: Self::State, _message: &Self::Operation) -> Result<Self::State, Self::Error> {
        // We shift "dependency checked" message ordering to outside of `p2panda-spaces`.
        unreachable!()
    }

    fn next_ready_message(
        _y: Self::State,
    ) -> Result<(Self::State, Option<Self::Operation>), Self::Error> {
        // We shift "dependency checked" message ordering to outside of `p2panda-spaces`.
        unreachable!()
    }
}
