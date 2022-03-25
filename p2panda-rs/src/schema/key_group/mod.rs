// SPDX-License-Identifier: AGPL-3.0-or-later

//! [`KeyGroup`] offers a way to group a set of public keys so that they can act as a single identity.
//!
//! Keys can only be added to a key group with a confirmation from both the key itself and an
//! existing member key. Key groups can also be extended with other key groups, which extends the
//! set of keys in the former with those from the latter.
//!
//! [`Owner`] fields on documents place ownership in the key group pointed at.
mod error;
#[allow(clippy::module_inception)]
mod key_group;
mod membership;
mod membership_request;
mod owner;
#[cfg(test)]
mod tests;

pub use error::KeyGroupError;
pub use key_group::{KeyGroup, KeyGroupView};
pub use membership::{Membership, MembershipView};
pub use membership_request::MembershipRequestView;
pub use owner::Owner;
