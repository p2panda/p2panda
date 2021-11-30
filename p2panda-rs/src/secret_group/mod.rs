// SPDX-License-Identifier: AGPL-3.0-or-later

//! Create and moderate large secret groups to maintain key material for secure messaging.
//!
//! p2panda uses the [MLS] (Messaging Layer Security) protocol for group key negotiation to
//! establish secrets in a large group of users for "Sender Ratchet Secrets" or "Long Term
//! Secrets". Both settings give confidentiality, authenticity and post-compromise security, while
//! the sender ratchet scheme also gives forward secrecy.
//!
//! A group of users sharing that secret state is called a "secret group" in p2panda. Sender
//! ratchet encryption is interesting for applications with high security standards where every
//! message is individually protected with an epheremal key, whereas long term secret encryption is
//! useful for building application where keys material is reused for multiple messages over longer
//! time, so past data can still be decrypted, even when a member joins the secret group later.
//!
//! [MLS]: https://messaginglayersecurity.rocks
//!
//! ## Example
//!
//! ```
//! # extern crate p2panda_rs;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # use std::convert::TryFrom;
//! # use p2panda_rs::hash::Hash;
//! # use p2panda_rs::identity::KeyPair;
//! # use p2panda_rs::secret_group::{SecretGroup, SecretGroupMember, MlsProvider};
//! # let group_instance_id = Hash::new_from_bytes(vec![1, 2, 3])?;
//! // Define provider for cryptographic methods and key storage
//! let provider = MlsProvider::new();
//!
//! // Generate new Ed25519 key pair
//! let key_pair = KeyPair::new();
//!
//! // Create new group member based on p2panda key pair
//! let member = SecretGroupMember::new(&provider, &key_pair)?;
//!
//! // Create a secret group with member as the owner
//! let mut group = SecretGroup::new(&provider, &group_instance_id, &member)?;
//!
//! // Encrypt and decrypt message again for the only member
//! let ciphertext = group.encrypt(&provider, b"Secret Message")?;
//! let plaintext = group.decrypt(&provider, &ciphertext)?;
//! assert_eq!(plaintext, b"Secret Message");
//! # Ok(())
//! # }
//! ```
mod codec;
mod commit;
mod error;
mod group;
mod lts;
mod member;
mod message;
mod mls;
#[cfg(test)]
mod tests;

pub use commit::SecretGroupCommit;
pub use error::SecretGroupError;
pub use group::SecretGroup;
pub use member::SecretGroupMember;
pub use message::SecretGroupMessage;
// @TODO: This will be removed as soon as we have our own provider
pub use mls::MlsProvider;
