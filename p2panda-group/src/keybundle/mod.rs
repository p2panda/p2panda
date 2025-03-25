// SPDX-License-Identifier: MIT OR Apache-2.0

//! Key bundles a member can publish in the network to asynchronously receive encrypted data from
//! others.
//!
//! This is for asynchronous settings where one user ("Bob") is offline but has published key
//! bundle (their identity key, pre keys, etc.) beforehands. Another user ("Alice") wants to use
//! that information to send encrypted data to Bob.
//!
//! Depending on the security of the chat group this key bundle should be used only once with
//! [`OneTimeKeyBundle`] or only within a given "lifetime" (one day, one week, etc.) with
//! [`LongTermKeyBundle`]. Members need to make sure that there is always fresh key bundles from
//! them available in the network for others.
//!
//! ## Forward secrecy
//!
//! Some applications might have very strong forward secrecy requirements and only allow “one-time”
//! pre-keys per group. This means that we can only establish a Forward-Secure (FS) communication
//! channel with a peer if we reliably made sure to only use the pre-key exactly once. This is hard
//! to guarantee in a decentralised setting. If we don’t care about very strong FS we can ease up
//! on that requirement a little bit and tolerate re-use with longer-living pre-keys which get
//! rotated frequently (every week for example).
//!
//! ## Public-key infrastructure (PKI)
//!
//! We assume that encrypted groups with strong FS guarantees only get established when peers have
//! explicitly exchanged their one-time pre-keys with each other, for example in form of scanning
//! QR codes.
//!
//! Another solution for very strong forward secrecy, where we can make sure the pre-key is only
//! used once, is a "bilateral session state establishment" process where peers can only establish
//! a group chat with each other after both parties have been online. They don’t need to be online
//! at the same time, just to be online at least once and receive the messages of the other party.
//! This puts a slight restriction on the "offline-first" nature for peer-to-peer applications.
//!
//! Another solution is to rely on always-online and trusted key servers which maintain the
//! pre-keys for the network, but this puts an unnecessary centralisation point into the system and
//! seems even worse. Publishing pre-keys via DNS might be an interesting solution to look into.
//!
//! For longer-living ("long-term") pre-key material peers can regularily publish fresh key bundles
//! on the network, other peers need to make sure they keep collecting the latest bundles
//! regularily.
#[allow(clippy::module_inception)]
mod keybundle;
mod lifetime;
mod prekey;

pub use keybundle::{KeyBundleError, LongTermKeyBundle, OneTimeKeyBundle};
pub use lifetime::{Lifetime, LifetimeError};
pub use prekey::{OneTimeKey, OneTimeKeyId, PreKey};
