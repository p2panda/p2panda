// SPDX-License-Identifier: AGPL-3.0-or-later

//! p2panda uses the MLS (Messaging Layer Security) protocol for secure group key negotiation to
//! establish secrets in a group of users for asymmetric (DHKEMX25519 and AES128GCM) or symmetric
//! (AES256 with GCM-SIV) encryption schemes. Both settings allow post-compromise security, while
//! the asymmetric setting also gives forward secrecy.
pub(crate) mod aes;
mod commit;
mod error;
mod group;
pub(crate) mod lts;
mod member;
mod message;
pub(crate) mod mls;
#[cfg(test)]
mod tests;

pub use commit::SecretGroupCommit;
pub use error::SecretGroupError;
pub use group::SecretGroup;
pub use member::SecretGroupMember;
pub use message::SecretGroupMessage;
