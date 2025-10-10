// SPDX-License-Identifier: MIT OR Apache-2.0

//! Trait interfaces expressing signed space messages.
use crate::message::SpacesArgs;
use crate::{ActorId, OperationId};

// @TODO: Use traits from p2panda-core when ready:
// https://github.com/p2panda/p2panda/blob/a6762e9831ccc8b6c008caf655468d6e75cff408/p2panda-core/src/traits.rs
pub trait AuthoredMessage {
    fn id(&self) -> OperationId;

    fn author(&self) -> ActorId;
}

pub trait SpacesMessage<ID, C> {
    fn args(&self) -> &SpacesArgs<ID, C>;
}
