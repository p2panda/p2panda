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

use crate::Credentials;
use crate::forge::OperationForge;
use crate::spaces::types::{AuthCapabilities, SpacesManager};

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

// TODO: Put this behind a flag as soon as we don't use the in-memory stores.
// #[cfg(any(test, feature = "test_utils"))]
pub async fn test_spaces_manager(
    forge: OperationForge,
    credentials: Credentials,
) -> Result<SpacesManager, SpacesManagerError> {
    use p2panda_encryption::Rng;
    use p2panda_spaces::test_utils::TestKeyStore;

    use crate::spaces::types::{SpacesManager, TestSpacesStore};

    let rng = Rng::default();
    let store = TestSpacesStore::new();
    let key_store = TestKeyStore::new();

    SpacesManager::new(store, key_store, forge.clone(), (&credentials).into(), rng).await
}

pub(crate) fn actor_to_topic(actor_id: impl Into<ActorId>) -> Topic {
    actor_id.into().as_bytes().to_owned().into()
}

pub(crate) fn to_initial_members(
    initial_members: &[(ActorId, AccessLevel)],
) -> Vec<(ActorId, Access<AuthCapabilities>)> {
    initial_members
        .iter()
        .map(|(actor, level)| {
            (
                *actor,
                Access {
                    conditions: None,
                    level: level.clone(),
                },
            )
        })
        .collect()
}
