// SPDX-License-Identifier: MIT OR Apache-2.0

//! `p2panda-encryption` provides decentralized, secure data- and message encryption for groups
//! with post-compromise security and optional forward secrecy.
//!
//! The crate implements two different group key-agreement and encryption schemes for a whole range
//! of use cases for applications which can't rely on a stable network connection or centralised
//! coordination.
//!
//! The first scheme we simply call [**"Data Encryption"**](data_scheme), allowing peers to encrypt any data with
//! a secret, symmetric key for a group (using XChaCha20-Poly1305). This will be useful for building
//! applications where users who enter a group late will still have access to previously-created
//! content, for example knowledge databases, wiki applications or a booking tool for rehearsal
//! rooms.
//!
//! A member will not learn about any newly-created data after they are removed from the group,
//! since the key gets rotated on member removal. This should accommodate for many use-cases in p2p
//! applications which rely on basic group encryption with post-compromise security (PCS) and
//! forward secrecy (FS) during key agreement. Applications can optionally choose to remove
//! encryption keys for forward secrecy if they so desire.
//!
//! The second scheme is [**"Message Encryption"**](message_scheme), offering a forward secure (FS)
//! messaging ratchet, similar to Signal's [Double Ratchet
//! algorithm](https://en.wikipedia.org/wiki/Double_Ratchet_Algorithm). Since secret keys are
//! always generated for each message, a user can not easily learn about previously-created
//! messages when getting hold of such a key. We believe that the latter scheme will be used in
//! more specialised applications, for example p2p group chats, as strong forward secrecy comes
//! with it's own UX requirements. We are nonetheless excited to offer a solution for both worlds,
//! depending on the application's needs.
//!
//! More detail about the particular implementation and design choices of `p2panda-encryption` can
//! be found in our [in-depth blog post](https://p2panda.org/2025/02/24/group-encryption.html) and
//! [README](https://github.com/p2panda/p2panda/blob/main/p2panda-encryption/README.md).
pub mod crypto;
#[cfg(any(test, feature = "data_scheme"))]
pub mod data_scheme;
pub mod key_bundle;
pub mod key_manager;
pub mod key_registry;
#[cfg(any(test, feature = "message_scheme"))]
pub mod message_scheme;
#[cfg(any(test, feature = "test_utils"))]
mod ordering;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
pub mod traits;
pub mod two_party;

pub use crypto::{Rng, RngError};
