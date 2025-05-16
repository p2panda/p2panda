// SPDX-License-Identifier: MIT OR Apache-2.0

//! `p2panda-encryption` provides decentralized secure data- and message encryption for groups with
//! post-compromise security and optional forward secrecy.
//!
//! This implementation is compatible with any data type, encoding format or transport, made for
//! p2p applications which do not rely on constant internet connectivity. Similar to our other
//! p2panda crates, we aim to make our implementation "framework independent" while providing
//! optional "glue code" to integrate it in into the larger [p2panda
//! ecosystem](https://p2panda.org).
//!
//! More detail about the particular implementation and design choices of `p2panda-encryption` can
//! be found in our [in-depth blog post](https://p2panda.org/2025/02/24/group-encryption.html).
//!
//! ## Two encryption schemes
//!
//! `p2panda-encryption` offers two different group key-agreement and encryption schemes. The first
//! scheme we simply call **"Data Encryption"**, allowing peers to encrypt any data with a secret,
//! symmetric key for a group (XChaCha20-Poly1305). This will be useful for building applications
//! where users who enter a group late will still have access to previously created content, for
//! example private knowledge or wiki applications or a booking tool for rehearsal rooms. A member
//! will not learn about any newly created data after removing them from the group since the key
//! gets rotated on member removal or manual key update. This should accommodate for many use-cases
//! in p2p applications which rely on basic group encryption with post-compromise security (PCS)
//! and forward secrecy (FS) during key agreement.
//!
//! The second scheme is **"Message Encryption"**, offering a forward-secure (FS) messaging
//! ratchet, similar to Signal’s [Double Ratchet
//! algorithm](https://en.m.wikipedia.org/wiki/Double_Ratchet_Algorithm). Since secret keys are
//! always generated for each message, a user can not easily learn about previously created
//! messages when getting hold of such key. We believe that the latter scheme will be used in more
//! specialised applications, for example p2p group chats, as strong forward-secrecy comes with
//! it's own UX requirements, but we are excited to offer a solution for both worlds, depending on
//! the application's needs.
//!
//! ## Secure key-agreement
//!
//! To encrypt any data towards a group we need to first securely and efficiently make all members
//! of the group aware of the secret key which was used to encrypt the message. This takes place
//! inside a "key agreement" protocol which is the heart of `p2panda-encryption`.
//!
//! Both schemes use the Two-Party Secure Messaging (2SM) Key Agreement Protocol as specified in
//! the paper ["Key Agreement for Decentralized Secure Group Messaging with Strong Security
//! Guarantees"](https://eprint.iacr.org/2020/1281.pdf>) (2020).
//!
//! During the initial 2SM "round" using X3DH the forward-secrecy is defined by the lifetime of the
//! used pre-keys. For strong security guarantees it is recommended to use one-time pre-keys. If
//! this requirement can be relaxed it is possible to use long-term pre-keys, with a lifetime
//! defined by the application.
//!
//! Each subsequent 2SM HPKE round uses exactly one secret key, which is then dropped and replaced
//! by a newly-generated key-pair. This gives the key-agreement protocol strong forward secrecy
//! guarantees for each round, independent of the used pre-keys.
//!
//! ## Robustness in decentralized systems
//!
//! `p2panda-encryption` has been specifically designed to be robust when used in decentralized
//! systems. It accounts for use in scenarios without guaranteed connectivity between members of
//! the group and corner cases where group changes (adding, removing members etc.) take place
//! concurrently. No centralised server is required for coordination of the group.
//!
//! ## Usage & integration
//!
//! There are two options to use `p2panda-encryption`. One is to use the fully integrated p2panda
//! stack which gives an already complete and tested end-to-end solution for building secure,
//! decentralized applications with p2panda data types. If you're interested in group encryption
//! for your application but not building the "p2p backend", this is for you.
//!
//! Check out this examples for a simple chat application which uses the p2panda stack and
//! `p2panda-encryption`. TODO
//!
//! The second option comes with more flexibility if you're interested in integrating group
//! encryption into your custom p2p data-types and algorithms but also requires more care around
//! message ordering (partial ordering), group management (CRDT), validation (additional checks
//! around pre-key lifetimes, etc.) and authentication (signatures). Integrating such systems is
//! not trivial because great care is required around correct message ordering, validation and
//! authentication. We've tried to reduce the API surface for integrations into custom applications
//! as much as possible. If you struggle, please [reach out](https://p2panda.org/#contact).
//!
//! To get an overview of what is required for a custom integration we recommended to check out
//! the high-level APIs for [data encryption](crate::data_scheme::Group) and [message
//! encryption](crate::message_scheme::Group).
//!
//! ## Security
//!
//! Encryption helps to prevent your data being readable by third parties but it can never
//! guarantee full security, especially in decentralized, experimental networks.
//!
//! We can not recommend using this technology for high-risk use-cases when you can not guarantee
//! full control over all devices and transport channels. We recommend
//! [DeltaChat](https://delta.chat/en/) or good old [Signal](https://signal.org/) if you are
//! concerned about legal implications of your work.
//!
//! ### Audit
//!
//! `p2panda-encryption` received an security audit in June 2025 by [Radically Open
//! Security](https://www.radicallyopensecurity.com/). TODO: Link to report, results, etc.
//! sponsored by NLNet
//!
//! ### Meta-Data
//!
//! In the current implementation all group control messages are _not_ encrypted. While application
//! data is fully protected, an adversary who got access to the network, will be able to observe
//! control messages and reason about which members are inside the group. The cryptographic
//! identities in the group are not necessarily connected to any concrete persons but can reveal
//! enough meta-data and patterns.
//!
//! We're working on a variant of `p2panda-encryption` where even control messages, sender and
//! recipient info are encrypted. This unfortunately comes with worse performance and special UX
//! requirements but we still believe there is a use-case for smaller groups.
//!
//! ### Post-Quantum
//!
//! While a future of post-quantum computers seems far, `p2panda-encryption` is not secure against
//! so called harvest-now-decrypt-later (HNDL) quantum adversaries as we're not using any
//! post-quantum-ready cryptography.
//!
//! ## Credits
//!
//! We have been particularly inspired by the ["Key Agreement for Decentralized Secure Group
//! Messaging with Strong Security Guarantees"](https://eprint.iacr.org/2020/1281.pdf) (DCGKA)
//! paper by Matthew Weidner, Martin Kleppmann, Daniel Hugenroth and Alastair R. Beresford
//! (published in 2021) which is the first paper we are aware of which introduces a PCS and FS
//! encryption scheme with a local-first mindset. On top there's already an almost complete [Java
//! implementation](https://github.com/trvedata/key-agreement) of the paper, which helped with
//! realising our Rust version.
//!
//! The paper formed the initial starting point of our work. In particular, we followed the
//! Double-Ratchet "Message Encryption" scheme with some improvements around managing group
//! membership. We also carried over some of the ideas in the paper to accommodate for the simpler
//! "Data Encryption" approach.
//!
//! Our implementation uses Signal's [X3DH](https://signal.org/docs/specifications/x3dh)
//! key-agreement for initial rounds. This includes Signal's work around the
//! [XEdDSA](https://signal.org/docs/specifications/xeddsa) signature schemes.
mod crypto;
#[cfg(feature = "data_scheme")]
pub mod data_scheme;
mod key_bundle;
mod key_manager;
mod key_registry;
#[cfg(feature = "message_scheme")]
pub mod message_scheme;
mod ordering;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
pub mod traits;
mod two_party;

pub use crypto::{Rng, RngError};
pub use key_bundle::{
    Lifetime, LifetimeError, LongTermKeyBundle, OneTimeKeyBundle, OneTimePreKey, OneTimePreKeyId,
};
pub use key_manager::{KeyManager, KeyManagerError, KeyManagerState};
pub use key_registry::{KeyRegistry, KeyRegistryState};
pub use two_party::{
    LongTermTwoParty, OneTimeTwoParty, TwoParty, TwoPartyCiphertext, TwoPartyError, TwoPartyMessage,
};
