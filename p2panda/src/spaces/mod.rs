// SPDX-License-Identifier: MIT OR Apache-2.0

mod forge;
mod group;
mod member;
pub(crate) mod message;
mod space;
pub(crate) mod types;

// Re-export useful types.
pub use p2panda_auth::AccessLevel;
pub use p2panda_spaces::ActorId;
pub use p2panda_spaces::manager::ManagerError;

pub use group::{Group, GroupError, GroupFuture};
pub use member::{GroupActor, Member, MemberError};
pub(crate) use space::spaces_stream;
pub use space::{Space, SpaceError, SpaceFuture, SpaceSubscription};
pub use types::SpacesManagerError;
