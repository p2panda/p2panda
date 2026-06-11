// SPDX-License-Identifier: MIT OR Apache-2.0

mod forge;
mod group;
mod member;
pub(crate) mod message;
mod space;
pub(crate) mod types;

use std::fmt::Display;
use std::hash::Hash as StdHash;

use p2panda_auth::Access;
use p2panda_core::Topic;
use serde::{Deserialize, Serialize};

// Re-export useful types.
pub use p2panda_auth::AccessLevel;
pub use p2panda_spaces::ActorId;
pub use p2panda_spaces::manager::ManagerError;

pub use group::{Group, GroupError, GroupFuture};
pub use member::{GroupActor, Member, MemberError};
pub(crate) use space::spaces_stream;
pub use space::{Space, SpaceError, SpaceFuture, SpaceSubscription};
pub use types::SpacesManagerError;

pub const SPACE_ID_LENGTH: usize = 32;

#[derive(Copy, Clone, Debug, PartialEq, Eq, StdHash, Serialize, Deserialize)]
pub struct SpaceId(Topic);

impl p2panda_spaces::traits::SpaceId for SpaceId {}

impl SpaceId {
    pub fn random() -> Self {
        Self(Topic::random())
    }

    pub fn as_bytes(&self) -> &[u8; SPACE_ID_LENGTH] {
        self.0.as_bytes()
    }

    pub fn to_bytes(self) -> [u8; SPACE_ID_LENGTH] {
        self.0.to_bytes()
    }

    pub fn to_hex(&self) -> String {
        self.0.to_hex()
    }
}

impl From<SpaceId> for Topic {
    fn from(space_id: SpaceId) -> Self {
        Topic::from(space_id.to_bytes())
    }
}

impl From<Topic> for SpaceId {
    fn from(topic: Topic) -> Self {
        Self(topic)
    }
}

impl Display for SpaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}
