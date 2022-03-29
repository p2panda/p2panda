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
mod owner;
#[cfg(test)]
mod tests;
mod views;

pub use error::KeyGroupError;
pub use key_group::KeyGroup;
pub use membership::Membership;
pub use owner::Owner;
pub use views::key_group::KeyGroupView;
pub use views::request::MembershipRequestView;
pub use views::response::MembershipResponseView;
