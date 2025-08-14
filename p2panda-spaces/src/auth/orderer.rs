// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;

use p2panda_auth::traits::Conditions;

use crate::auth::message::{AuthArgs, AuthMessage};
use crate::types::{ActorId, AuthControlMessage, OperationId};

// Manages "dependencies" required for `p2panda-auth`.
#[derive(Clone, Debug)]
pub struct AuthOrderer {}

impl Default for AuthOrderer {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthOrderer {
    pub fn new() -> Self {
        Self {}
    }
}

impl<C> p2panda_auth::traits::Orderer<ActorId, OperationId, AuthControlMessage<C>> for AuthOrderer
where
    C: Conditions,
{
    type State = (); // @TODO

    type Operation = AuthMessage<C>;

    type Error = Infallible; // @TODO

    fn next_message(
        y: Self::State,
        control_message: &AuthControlMessage<C>,
    ) -> Result<(Self::State, Self::Operation), Self::Error> {
        // @TODO: we aren't focussing on ordering now so no dependencies are required, when we
        // introduce ordering then auth dependencies should be calculated and returned here.
        Ok((
            y,
            AuthMessage::Args(AuthArgs {
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
