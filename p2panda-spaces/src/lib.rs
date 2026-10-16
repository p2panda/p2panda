// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(docsrs, feature(doc_cfg))]
#![allow(
    clippy::result_large_err,
    reason = "https://github.com/p2panda/p2panda/issues/1458"
)]

mod config;
mod credentials;
mod encryption;
mod event;
mod forge;
pub mod group;
pub mod identity;
pub mod manager;
pub mod member;
mod message;
mod pending;
pub mod space;
mod store;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
#[cfg(test)]
mod tests;
mod types;
mod utils;
mod waiters;

use std::sync::OnceLock;

use p2panda_core::{Hash, VerifyingKey};

pub use config::Config;
pub use credentials::Credentials;
pub use event::{Event, GroupActor, GroupContext, GroupEvent, SpaceContext, SpaceEvent};
pub use forge::Forge;
pub use message::{AuthMessage, SpacesArgs, SpacesMessage};
pub use pending::{PendingGroup, PendingMember, PendingSpace};
pub use store::SpacesStoreState;
pub use types::{AuthGroupState, StrongRemoveResolver};

pub type SpaceId = Hash;

pub type GroupId = ActorId;

pub type MemberId = ActorId;

pub type ActorId = VerifyingKey;

pub type OperationId = Hash;

pub fn global_groups_id() -> Hash {
    static DEFAULT_GLOBAL_GROUPS_ID: OnceLock<Hash> = OnceLock::new();
    DEFAULT_GLOBAL_GROUPS_ID
        .get_or_init(|| Hash::digest(b"global-groups-context"))
        .to_owned()
}
