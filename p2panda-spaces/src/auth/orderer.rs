// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;

use p2panda_auth::traits::Conditions;

use crate::auth::message::{AuthArgs, AuthMessage};
use crate::types::{ActorId, AuthControlMessage, OperationId};

#[derive(Debug)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct AuthOrdererState {
    pub dependencies: Vec<OperationId>,
}

// Manages "dependencies" required for `p2panda-auth`.
#[derive(Clone, Debug)]
pub struct AuthOrderer {}

impl AuthOrderer {
    pub fn init() -> AuthOrdererState {
        AuthOrdererState {
            dependencies: Default::default(),
        }
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
        let dependencies = y.dependencies.clone();
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
