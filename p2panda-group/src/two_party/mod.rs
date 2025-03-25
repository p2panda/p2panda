// SPDX-License-Identifier: MIT OR Apache-2.0

//! Protocols for secure 1:1 communication between two members ("two party").
//!
//! Group encryption is concerned around delivering an encrypted message from a single sender to a
//! whole group with multiple receivers.
//!
//! To achieve this "broadcast" nature of such system we still need pair-wise communication between
//! members of the group (one single sender to one single receiver), for example to establish
//! secret key material.
//!
//! A "two party secure messaging" protocol (2SM in short) is concerned around exactly that 1:1
//! secure communication.
mod key_bundle;
mod x3dh;

pub use key_bundle::{LongTermKeyBundle, OneTimeKey, OneTimeKeyBundle, PreKey};
pub use x3dh::X3DHError;
