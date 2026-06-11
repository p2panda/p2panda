// SPDX-License-Identifier: MIT OR Apache-2.0

mod forge;
pub(crate) mod message;
mod space;
pub(crate) mod types;

// Re-export useful types.
pub use p2panda_auth::AccessLevel;
pub use p2panda_spaces::ActorId;
pub use p2panda_spaces::manager::ManagerError;

pub use space::{Space, SpaceError, SpaceFuture, SpaceSubscription};
pub use types::SpacesManagerError;
