// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use crate::message::SpacesArgs;
use crate::{ActorId, OperationId};

// @TODO: This could be an interesting trait for `p2panda-core`, next to another one where we
// declare dependencies.
pub trait AuthoredMessage: Debug {
    fn id(&self) -> OperationId;

    fn author(&self) -> ActorId;

    // @TODO: Do we need a method here to check the signature?
}

pub trait SpacesMessage<ID, C> {
    fn args(&self) -> &SpacesArgs<ID, C>;
}
