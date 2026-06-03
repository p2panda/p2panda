// SPDX-License-Identifier: MIT OR Apache-2.0

//! Trait interfaces expressing signed space messages.
use crate::{ActorId, OperationId};

// @TODO: Remove these; we'll use the `Borrow` method instead and define a concrete type.
// @TODO: Use traits from p2panda-core when ready:
// https://github.com/p2panda/p2panda/blob/a6762e9831ccc8b6c008caf655468d6e75cff408/p2panda-core/src/traits.rs
/// Interface to be implemented on custom message types which have a unique operation id and were
/// signed by an author.
pub trait AuthoredMessage {
    fn id(&self) -> OperationId;

    fn author(&self) -> ActorId;
}
