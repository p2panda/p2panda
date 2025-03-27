// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use crate::crypto::Secret;
use crate::key_bundle::OneTimeKeyBundle;
use crate::traits::{
    AckedGroupMembership, IdentityHandle, IdentityManager, IdentityRegistry, OperationId,
    PreKeyManager, PreKeyRegistry,
};
use crate::two_party::TwoPartyState;

/// 256-bit secret "outer" chain- and update key.
const RATCHET_KEY_SIZE: usize = 32;

/// A decentralized continuous group key agreement protocol (DCGKA) for p2panda's "message
/// encryption" scheme with strong forward-secrecy and post-compromise security.
///
/// The implementation follows the DCGKA protocol specified in the paper: "Key Agreement for
/// Decentralized Secure Group Messaging with Strong Security Guarantees" by Matthew Weidner,
/// Martin Kleppmann, Daniel Hugenroth, Alastair R. Beresford (2020).
///
/// DCGKA generates a sequence of update secrets for each group member, which are used as input to
/// a ratchet to encrypt/decrypt application messages sent by that member. Only group members learn
/// these update secrets, and fresh secrets are generated every time a user is added or removed, or
/// a PCS update is requested. The DCGKA protocol ensures that all users observe the same sequence
/// of update secrets for each group member, regardless of the order in which concurrent messages
/// are received.
///
/// ```text
///                ┌────────────────────────────────────────────────┐
///                │                 "Outer" Ratchet                │
///   Alice        ├────────────────────────────────────────────────┤
///     │          │                                                │
///     │          │                         Previous Chain Secret  │
///     │          │                               for Bob          │
/// Delivered      │                                                │
///  via 2SM       │                                  │             │
///     │          │                                  │             │
///     │          │                                  │             │   ┌─────┐
///     ▼          │                                  │ ◄───────────┼───│"Ack"│
///   ┌────┐       │                                  │             │   └─────┘
///   │Seed├───────│──►  HKDF                         │             │
///   └────┘       │      │           Bob             │             │
///                │      │     ┌─────────────┐       │             │
///                │      ├───► │Member Secret├───────┼─► HKDF ─────┼──► Update Secret
///                │      │     └─────────────┘       │             │        │
///                │      │                           ▼             │        │
///                │      │         Charlie          HKDF           │        │
///                │      │     ┌─────────────┐       │             │        │
///                │      ├───► │Member Secret├─...   │             │        ▼
///                │      │     └─────────────┘       │             │ ┌───────────────┐
///                │      │                           │             │ │Message Ratchet│
///                │      │           ...             │             │ └───────────────┘
///                │      │     ┌─────────────┐       │             │
///                │      └───► │Member Secret├─...   │             │
///                │            └─────────────┘       │             │
///                │                                  │             │
///                │                                  ▼             │
///                │                           New Chain Secret     │
///                │                                  │             │
///                │                                  │             │
///                │                                  ▼  ...        │
///                │                                                │
///                └────────────────────────────────────────────────┘
/// ```
///
/// To initiate a PCS update, a user generates a fresh random value called a seed secret, and sends
/// it to each other group member via a two-party secure channel, like in Sender Keys. On receiving
/// a seed secret, a group member deterministically derives from it an update secret for the
/// sender's ratchet, and also an update secret for its own ratchet. Moreover, the recipient
/// broadcasts an unencrypted acknowledgment to the group indicating that it has applied the
/// update. Every recipient of the acknowledgment then updates not only the ratchet for the sender
/// of the original update, but also the ratchet for the sender of the acknowledgment. Thus, after
/// one seed secret has been disseminated via n - 1 two-party messages, and confirmed via n - 1
/// broadcast acknowledgments, each group member has derived an update secret from it and updated
/// their ratchet.
///
/// <https://eprint.iacr.org/2020/1281.pdf>
pub struct Dcgka<ID, OP, PKI, DGM, MGT> {
    _marker: PhantomData<(ID, OP, PKI, DGM, MGT)>,
}

/// Serializable state of DCGKA (for persistance).
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(Clone))]
pub struct DcgkaState<ID, OP, PKI, DGM, MGT>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
    DGM: AckedGroupMembership<ID, OP>,
    MGT: IdentityManager<MGT::State> + PreKeyManager,
{
    /// Public Key Infrastructure. From here we retrieve the identity keys and one-time prekey
    /// bundles for each member to do 2SM.
    pki: PKI::State,

    /// Our own key mananger holding the secret parts for our own identity keys and published
    /// one-time prekey bundles so we can do 2SM.
    my_keys: MGT::State,

    /// Our id which is used as an unique handle.
    my_id: ID,

    /// Randomly generated seed we keep temporarily around when creating or updating a group or
    /// removing a member.
    next_seed: Option<NextSeed>,

    /// Handlers for each member to manage the "Two-Party Secure Messaging" (2SM) key-agreement
    /// protocol as specified in the paper.
    two_party: HashMap<ID, TwoPartyState<OneTimeKeyBundle>>, // "2sm" in paper

    /// Member secrets are "temporary" secrets we derive after receiving a new seed or adding
    /// someone. We keep them around until we've received an acknowledgment of that member.
    ///
    /// We only store the member secrets, and not the seed secret, so that if the user’s private
    /// state is compromised, the adversary obtains only those member secrets that have not yet
    /// been used.
    ///
    /// The first parameter in the key tuple is the "sender" or "original creator" of the update
    /// secret. They generated the secret during the given "sequence" (second parameter) for a
    /// "member" (third parameter).
    member_secrets: HashMap<(ID, OP, ID), ChainSecret>, // key: "(sender, seq, ID)" in paper

    /// Chain secrets for the "outer" key-agreement ratchet.
    ///
    /// Secrets for the "inner" message ratchet are returned to the user as part of the
    /// "sender_update_secret" and "me_update_secret" fields when invoking a group membership
    /// operation or processing a control message.
    ratchet: HashMap<ID, ChainSecret>,

    /// Decentralised group membership algorithm.
    dgm: DGM::State,
}

impl<ID, OP, PKI, DGM, MGT> Dcgka<ID, OP, PKI, DGM, MGT>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
    DGM: AckedGroupMembership<ID, OP>,
    MGT: IdentityManager<MGT::State> + PreKeyManager,
{
    pub fn init(
        my_id: ID,
        my_keys: MGT::State,
        pki: PKI::State,
        dgm: DGM::State,
    ) -> DcgkaState<ID, OP, PKI, DGM, MGT> {
        DcgkaState {
            pki,
            my_id,
            my_keys,
            next_seed: None,
            two_party: HashMap::new(),
            member_secrets: HashMap::new(),
            ratchet: HashMap::new(),
            dgm,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(Clone))]
pub struct NextSeed(Secret<RATCHET_KEY_SIZE>);

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(Clone))]
pub struct ChainSecret(Secret<RATCHET_KEY_SIZE>);
