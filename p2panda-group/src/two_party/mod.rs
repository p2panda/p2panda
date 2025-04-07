// SPDX-License-Identifier: MIT OR Apache-2.0

//! Protocols for secure key-agreement between two members ("two party").
//!
//! Group encryption is concerned around delivering a secure message from a single sender to a
//! whole group with potentially many receivers. To achieve this "broadcast" topology of such
//! system we need to establish secret key material with each member of the group before. This
//! takes places in form of a pair-wise, secure key-agreement protocol between two members of the
//! group.
//!
//! The "two party secure messaging" key-agreement protocol (2SM) is specified in the paper "Key
//! Agreement for Decentralized Secure Group Messaging with Strong Security Guarantees" (2020).
//!
//! At its core, the 2SM protocol uses Public-Key Encryption (PKE) to encrypt messages, with
//! frequent key rotation to provide PCS and FS. Initially, each party encrypts messages using the
//! key-bundle input for X3DH. Afterwards, to achieve PCS, each time a party sends a message, it
//! also updates its public key. Finally, to avoid reusing public keys (which would make FS
//! impossible), whenever a party sends a message, it also updates the other party's public key. To
//! do so, it sends a new secret key along with its message, then deletes its own copy, storing
//! only the public key.
//!
//! <https://eprint.iacr.org/2020/1281.pdf>
#[allow(clippy::module_inception)]
mod two_party;
mod x3dh;

pub use two_party::{
    LongTermTwoParty, OneTimeTwoParty, TwoParty, TwoPartyCiphertext, TwoPartyError,
    TwoPartyMessage, TwoPartyState,
};
pub use x3dh::{X3dhCiphertext, X3dhError, x3dh_decrypt, x3dh_encrypt};
