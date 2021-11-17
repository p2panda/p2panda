// SPDX-License-Identifier: AGPL-3.0-or-later

//! p2panda uses the MLS (Messaging Layer Security) protocol for secure group key negotiation to
//! establish secrets in a group of users for asymmetric (DHKEMX25519 and AES128GCM) or symmetric
//! (AES256 with GCM-SIV) encryption schemes. Both settings allow post-compromise security, while
//! the asymmetric setting also gives forward secrecy.
mod commit;
mod group;
mod member;
mod message;
pub(crate) mod aes;
pub(crate) mod lts;
pub(crate) mod mls;

pub use commit::SecretGroupCommit;
pub use group::SecretGroup;
pub use member::SecretGroupMember;
pub use message::SecretGroupMessage;
