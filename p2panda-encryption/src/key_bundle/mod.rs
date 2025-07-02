// SPDX-License-Identifier: MIT OR Apache-2.0

//! Key bundles to asynchronously receive encrypted data from others.
//!
//! This is for asynchronous settings where one user ("Bob") is offline but has published a key
//! bundle (containing their identity key, pre-keys, etc.) beforehands. Another user ("Alice")
//! wants to use that information to send encrypted data to Bob.
//!
//! Depending on the security of the chat group this key bundle should be _used only once_ with
//! [`OneTimeKeyBundle`] or only within a given "lifetime" (one day, two weeks, etc.) with
//! [`LongTermKeyBundle`]. Members need to make sure that there are always fresh key bundles from
//! them available in the network for others.
//!
//! ## Forward secrecy
//!
//! Some applications might have very strong forward secrecy requirements and only allow "one-time"
//! pre-keys per group. This means that we can only establish a Forward Secure (FS) communication
//! channel with a peer if we reliably made sure to only use the pre-key exactly once. This is hard
//! to guarantee in a decentralised setting. If we don’t care about very strong FS we can ease up
//! on that requirement a little bit and tolerate re-use with longer-living pre-keys which get
//! rotated frequently (every week for example).
//!
//! ## Public-key infrastructure (PKI)
//!
//! We assume that encrypted groups with strong FS guarantees using one-time key bundles only get
//! established when peers have explicitly exchanged their one-time pre-keys with each other, for
//! example in form of scanning QR codes.
//!
//! Another solution for very strong forward secrecy, where we can make sure the pre-key is only
//! used once, is a "bilateral session state establishment" process where peers can only establish
//! a group chat with each other after both parties have been online. They don't need to be online
//! at the same time, just to be online at least once and receive the messages of the other party.
//! This puts a slight restriction on the "offline-first" nature for peer-to-peer applications.
//!
//! Another solution is to rely on always-online and trusted key servers which maintain the
//! pre-keys for the network, but this puts an unnecessary centralisation point into the system and
//! seems even worse.
//!
//! For longer-living ("long-term") pre-key bundles we can lift the strict "use once" requirement
//! and peers can regularly publish fresh key bundles on the network, other peers need to make
//! sure they keep collecting the latest bundles regularly.
//!
//! ## Authenticated Messaging
//!
//! Note that while pre-keys are signed, bundles should be part of an authenticated messaging
//! scheme where the whole payload (and thus it's lifetime and maybe one-time pre-key) is signed by
//! the same identity to prevent replay- and impersonation attacks.
//!
//! Otherwise attackers might be able to re-play the same pre-key with different lifetimes.
#[allow(clippy::module_inception)]
mod key_bundle;
mod lifetime;
mod prekey;

pub use key_bundle::{KeyBundleError, LongTermKeyBundle, OneTimeKeyBundle};
pub use lifetime::{Lifetime, LifetimeError};
pub use prekey::{OneTimePreKey, OneTimePreKeyId, PreKey};
