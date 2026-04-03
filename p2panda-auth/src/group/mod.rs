// SPDX-License-Identifier: MIT OR Apache-2.0

//! Group membership and authorisation.

mod action;
mod authority_graphs;
pub(crate) mod crdt;
#[cfg(any(test, feature = "test_utils"))]
mod display;
mod member;
pub mod resolver;

pub use action::GroupAction;
pub(crate) use authority_graphs::AuthorityGraphs;
pub(crate) use crdt::apply_action;
pub use crdt::state::{GroupMembersState, GroupMembershipError, MemberState};
pub use crdt::{
    GroupCrdt, GroupCrdtError, GroupCrdtInnerError, GroupCrdtInnerState, GroupCrdtState,
    StateChangeResult,
};
pub use member::GroupMember;
