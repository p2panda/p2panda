// SPDX-License-Identifier: MIT OR Apache-2.0

//! A decentralized continuous group key agreement protocol (DCGKA) for p2panda's "message
//! encryption" scheme with strong forward secrecy and post-compromise security.
//!
//! ## Protocol
//!
//! DCGKA generates a sequence of update secrets for each group member, which are used as input to
//! a ratchet to encrypt/decrypt application messages sent by that member. Only group members learn
//! these update secrets, and fresh secrets are generated every time a user is added or removed, or
//! a PCS update is requested. The DCGKA protocol ensures that all users observe the same sequence
//! of update secrets for each group member, regardless of the order in which concurrent messages
//! are received.
//!
//! ```text
//!                ┌────────────────────────────────────────────────┐
//!                │                 "Outer" Ratchet                │
//!   Alice        ├────────────────────────────────────────────────┤
//!     │          │                                                │
//!     │          │                         Previous Chain Secret  │
//!     │          │                               for Bob          │
//! Delivered      │                                                │
//!  via 2SM       │                                  │             │
//!     │          │                                  │             │
//!     │          │                                  │             │   ┌─────┐
//!     ▼          │                                  │ ◄───────────┼───│"Ack"│
//!   ┌────┐       │                                  │             │   └─────┘
//!   │Seed├───────│──►  HKDF                         │             │
//!   └────┘       │      │           Bob             │             │
//!                │      │     ┌─────────────┐       │             │
//!                │      ├───► │Member Secret├───────┼─► HKDF ─────┼──► Update Secret
//!                │      │     └─────────────┘       │             │        │
//!                │      │                           ▼             │        │
//!                │      │         Charlie          HKDF           │        │
//!                │      │     ┌─────────────┐       │             │        │
//!                │      ├───► │Member Secret├─...   │             │        ▼
//!                │      │     └─────────────┘       │             │ ┌───────────────┐
//!                │      │                           │             │ │Message Ratchet│
//!                │      │           ...             │             │ └───────────────┘
//!                │      │     ┌─────────────┐       │             │
//!                │      └───► │Member Secret├─...   │             │
//!                │            └─────────────┘       │             │
//!                │                                  │             │
//!                │                                  ▼             │
//!                │                           New Chain Secret     │
//!                │                                  │             │
//!                │                                  │             │
//!                │                                  ▼  ...        │
//!                │                                                │
//!                └────────────────────────────────────────────────┘
//! ```
//!
//! To initiate a PCS update, a user generates a fresh random value called a seed secret, and sends
//! it to each other group member via a two-party secure channel, like in Sender Keys. On receiving
//! a seed secret, a group member deterministically derives from it an update secret for the
//! sender's ratchet, and also an update secret for its own ratchet. Moreover, the recipient
//! broadcasts an unencrypted acknowledgment to the group indicating that it has applied the
//! update. Every recipient of the acknowledgment then updates not only the ratchet for the sender
//! of the original update, but also the ratchet for the sender of the acknowledgment. Thus, after
//! one seed secret has been disseminated via n - 1 two-party messages, and confirmed via n - 1
//! broadcast acknowledgments, each group member has derived an update secret from it and updated
//! their ratchet.
//!
//! ## Credits
//!
//! The implementation follows the DCGKA protocol specified in the paper: "Key Agreement for
//! Decentralized Secure Group Messaging with Strong Security Guarantees" by Matthew Weidner,
//! Martin Kleppmann, Daniel Hugenroth, Alastair R. Beresford (2020).
//!
//! <https://eprint.iacr.org/2020/1281.pdf>
//!
//! Some adjustments have been made to the version in the paper:
//!
//! * Renamed `process` to `process_remote`.
//! * Added `rng` as an argument to most methods.
//! * `seq` is taken care of _outside_ of this implementation. Methods return control messages
//!   which need to be manually assigned a "seq", that is a vector clock, hash, seq_num or similar.
//! * After calling a group operation "create", "add", "remove" or "update" the user needs to process
//!   the output themselves by calling `process_local`. This allows a user of the API to correctly
//!   craft a `seq` for their control messages (see point above).
//! * Instead of sending the history of control messages in "welcome" messages we send the
//!   "processed" and potentially garbage-collected CRDT state of DGM. This also allows
//!   implementations where control messages are encrypted as well.
//! * We're not recording the "add" control message to the history before sending a "welcome"
//!   message after adding a member, the receiver of the "welcome" message needs to add themselves.
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::hkdf::{HkdfError, hkdf};
use crate::crypto::{Rng, RngError, Secret};
use crate::key_bundle::OneTimeKeyBundle;
use crate::traits::{
    AckedGroupMembership, IdentityHandle, IdentityManager, IdentityRegistry, OperationId,
    PreKeyManager, PreKeyRegistry,
};
use crate::two_party::{TwoParty, TwoPartyError, TwoPartyMessage, TwoPartyState};

/// 256-bit secret "outer" chain- and update key.
const RATCHET_KEY_SIZE: usize = 32;

/// A decentralized continuous group key agreement protocol (DCGKA) for p2panda's "message
/// encryption" scheme with strong forward secrecy and post-compromise security.
pub struct Dcgka<ID, OP, PKI, DGM, KMG> {
    _marker: PhantomData<(ID, OP, PKI, DGM, KMG)>,
}

/// Serializable state of DCGKA (for persistence).
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct DcgkaState<ID, OP, PKI, DGM, KMG>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
    DGM: AckedGroupMembership<ID, OP>,
    KMG: IdentityManager<KMG::State> + PreKeyManager,
{
    /// Public Key Infrastructure (PKI). From here we retrieve the identity keys and one-time
    /// prekey bundles for each member to do 2SM.
    pub(crate) pki: PKI::State,

    /// Our own key manager state holding the secret parts for our own identity keys and published
    /// one-time prekey bundles so we can do 2SM.
    pub(crate) my_keys: KMG::State,

    /// Our id which is used as a unique handle inside this group.
    pub(crate) my_id: ID,

    /// Randomly generated seed we keep temporarily around when creating or updating a group or
    /// removing a member.
    pub(crate) next_seed: Option<NextSeed>,

    /// Handlers for each member to manage the "Two-Party Secure Messaging" (2SM) key-agreement
    /// protocol as specified in the paper.
    pub(crate) two_party: HashMap<ID, TwoPartyState<OneTimeKeyBundle>>, // "2sm" in paper

    /// Member secrets are "temporary" secrets we derive after receiving a new seed or adding
    /// someone. We keep them around until we've received an acknowledgment of that member.
    ///
    /// We only store the member secrets, and not the seed secret, so that if the user's private
    /// state is compromised, the adversary obtains only those member secrets that have not yet
    /// been used.
    ///
    /// The first parameter in the key tuple is the "sender" or "original creator" of the update
    /// secret. They generated the secret during the given "sequence" (second parameter) for a
    /// "member" (third parameter).
    pub(crate) member_secrets: HashMap<(ID, OP, ID), ChainSecret>, // key: "(sender, seq, ID)" in paper

    /// Chain secrets for the "outer" key-agreement ratchet.
    ///
    /// Secrets for the "inner" message ratchet are returned to the user as part of the
    /// "sender_update_secret" and "me_update_secret" fields when invoking a group membership
    /// operation or processing a control message.
    pub(crate) ratchet: HashMap<ID, ChainSecret>,

    /// Decentralised group membership (DGM) state.
    pub(crate) dgm: DGM::State,
}

impl<ID, OP, PKI, DGM, KMG> Dcgka<ID, OP, PKI, DGM, KMG>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
    DGM: AckedGroupMembership<ID, OP>,
    KMG: IdentityManager<KMG::State> + PreKeyManager,
{
    /// Returns new DCGKA state with our own identity and key managers.
    ///
    /// Use this when creating a new group or before accepting an invitation to an existing one.
    pub fn init(
        my_id: ID,
        my_keys: KMG::State,
        pki: PKI::State,
        dgm: DGM::State,
    ) -> DcgkaState<ID, OP, PKI, DGM, KMG> {
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

    /// Handler for when a "remote" control message is received from the network.
    ///
    /// It takes the user ID of the message sender, a control message, and a direct message (or
    /// none if there is no associated direct message).
    ///
    /// Control messages are expected to be authenticated and causally ordered.
    pub fn process_remote(
        y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        input: ProcessInput<ID, OP, DGM>,
        rng: &Rng,
    ) -> DcgkaProcessResult<ID, OP, PKI, DGM, KMG> {
        let ProcessInput {
            sender,
            seq,
            direct_message,
            control_message,
        } = input;
        assert_ne!(sender, y.my_id, "do not process own control messages");
        let (y_i, output) = match control_message {
            ControlMessage::Create { initial_members } => {
                Self::process_create(y, sender, seq, initial_members, direct_message, rng)?
            }
            ControlMessage::Ack {
                ack_sender,
                ack_seq,
            } => Self::process_ack(y, sender, (&ack_sender, ack_seq), direct_message)?,
            ControlMessage::Update => Self::process_update(y, sender, seq, direct_message, rng)?,
            ControlMessage::Remove { removed } => {
                Self::process_remove(y, sender, seq, &removed, direct_message, rng)?
            }
            ControlMessage::Add { added } => {
                Self::process_add(y, sender, seq, added, direct_message, rng)?
            }
            ControlMessage::AddAck {
                ack_sender,
                ack_seq,
            } => Self::process_add_ack(y, sender, (&ack_sender, ack_seq), direct_message)?,
        };
        Ok((y_i, output))
    }

    /// Handler which is _always_ called _after_ every local group membership operation
    /// ("create", "update", "remove" or "add") which was applied by us.
    ///
    /// Invoking a membership operation always returns a "control message" which needs to be
    /// processed by the application _before_ we can process it locally (and thus finally updating
    /// our local state). This is because some applications have more complex requirements around
    /// wrapping the control message around their own message type which will be published on the
    /// network (for example an append-only log entry with signature, vector clocks, etc.) and we
    /// can't "guess" the resulting operation id.
    ///
    /// Calling this method will _never_ yield another control- or direct message.
    pub fn process_local(
        y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        seq: OP,
        input: OperationOutput<ID, OP, DGM>,
        rng: &Rng,
    ) -> DcgkaOperationResult<ID, OP, PKI, DGM, KMG> {
        let my_id = y.my_id;
        let (y_i, output) = match input.control_message {
            ControlMessage::Create {
                ref initial_members,
            } => Self::process_create(y, my_id, seq, initial_members.clone(), None, rng)?,
            ControlMessage::Update => Self::process_update(y, my_id, seq, None, rng)?,
            ControlMessage::Remove { removed } => {
                Self::process_remove(y, my_id, seq, &removed, None, rng)?
            }
            ControlMessage::Add { added } => Self::process_add(y, my_id, seq, added, None, rng)?,
            _ => panic!(
                "only call process_local after local create, update, remove or add operations"
            ),
        };

        // Processing our local group operations should never yield control or direct messages.
        assert!(output.control_message.is_none());
        assert!(output.direct_messages.is_empty());

        Ok((
            y_i,
            OperationOutput {
                control_message: input.control_message,
                direct_messages: input.direct_messages,
                me_update_secret: Some(output.sender_update_secret.unwrap()),
            },
        ))
    }

    /// Takes a set of users IDs (including us) and creates a new group with those members.
    ///
    /// Note that every member ID needs to be unique for this group.
    ///
    /// A group is created in three steps: 1. one user calls "create" and broadcasts a control
    /// message of type "create" (plus direct messages) to the initial members; 2. each member
    /// processes that message and broadcasts an "ack" control message; 3. each member processes
    /// the ack from each other member.
    pub fn create(
        y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        initial_members: Vec<ID>,
        rng: &Rng,
    ) -> DcgkaOperationResult<ID, OP, PKI, DGM, KMG> {
        // De-duplicate members.
        let mut initial_members: Vec<ID> =
            initial_members.into_iter().fold(Vec::new(), |mut acc, id| {
                if !acc.contains(&id) {
                    acc.push(id);
                }
                acc
            });

        // Add ourselves if the user hasn't done it yet.
        if !initial_members.contains(&y.my_id) {
            initial_members.push(y.my_id);
        }

        // The "create" function constructs the "create" control message.
        let control_message = ControlMessage::Create {
            initial_members: initial_members.clone(),
        };

        // Generate the set of direct messages to send.
        let (y_ii, direct_messages) = Self::generate_seed(y, &initial_members, rng)?;

        // It then calls process_create to process the control message for this user (as if it had
        // received the message) before returning.
        //
        // process_create returns a tuple after updating the state including an update secret I; we
        // use these and ignore the rest.
        Ok((
            y_ii,
            OperationOutput {
                control_message,
                direct_messages,
                me_update_secret: None,
            },
        ))
    }

    /// Called by group members when they receive the "create" message.
    fn process_create(
        mut y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        sender: ID,
        seq: OP,
        initial_members: Vec<ID>,
        direct_message: Option<DirectMessage<ID, OP, DGM>>,
        rng: &Rng,
    ) -> DcgkaProcessResult<ID, OP, PKI, DGM, KMG> {
        y.dgm =
            DGM::create(y.my_id, &initial_members).map_err(|err| DcgkaError::DgmOperation(err))?;
        Self::process_seed(y, &sender, seq, direct_message, rng)
    }

    /// Called by group members when they receive the "ack" message.
    ///
    /// In this function, `ackID` and `ackOP` are the user id and id of the acknowledged message.
    fn process_ack(
        mut y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        sender: ID,
        ack: (&ID, OP),
        direct_message: Option<DirectMessage<ID, OP, DGM>>,
    ) -> DcgkaProcessResult<ID, OP, PKI, DGM, KMG> {
        // If the acknowledged message was a group membership operation, we record the
        // acknowledgment. We do this because the member_view function needs to know which
        // operations have been acknowledged by which user.
        //
        // Acking the message will fail if it's an ack of the user's own removal. Thus we will
        // refuse to process messages from a user that depend on their own removal.
        if DGM::is_add(&y.dgm, ack.1) && DGM::is_remove(&y.dgm, ack.1) && sender != y.my_id {
            // This condition will fail for acks of the creation and of updates.
            y.dgm = DGM::ack(y.dgm, sender, ack.1).map_err(|err| DcgkaError::DgmOperation(err))?;
        }

        // Read from γ.memberSecret the appropriate member secret that was previously derived from
        // the seed secret in the message being acknowledged. The member secret is then deleted for
        // forward secrecy.
        let member_secret = y.member_secrets.remove(&(*ack.0, ack.1, sender)); // FS

        let (y_i, sender_member_secret) = match (member_secret, direct_message) {
            (None, None) => return Ok((y, ProcessOutput::default())),
            (Some(member_secret), _) => (y, member_secret),
            (
                None,
                Some(DirectMessage {
                    recipient,
                    content: DirectMessageContent::Forward { ciphertext },
                    ..
                }),
            ) => {
                if recipient != y.my_id {
                    // Direct message was not meant for us.
                    return Ok((y, ProcessOutput::default()));
                }

                // The recipient of such a message handles concurrent adds here, where the
                // forwarded member secret is decrypted and then used to update the ratchet for the
                // "ack" sender. Note that this forwarding behavior does not violate forward
                // secrecy: an application message can still only be decrypted by those users who
                // were group members at the time of sending.
                let (y_i, plaintext) = Self::decrypt_from(y, &sender, ciphertext)?;
                (y_i, ChainSecret::try_from_bytes(&plaintext)?)
            }
            (None, Some(direct_message)) => {
                return Err(DcgkaError::UnexpectedDirectMessageType(
                    DirectMessageType::Forward,
                    direct_message.message_type(),
                ));
            }
        };

        // We update the ratchet for the sender of the "ack" and return the resulting update
        // secret.
        let (y_ii, sender_update_secret) =
            Self::update_ratchet(y_i, &sender, sender_member_secret)?;

        Ok((
            y_ii,
            ProcessOutput {
                control_message: None,
                direct_messages: Vec::new(),
                sender_update_secret: Some(sender_update_secret),
                me_update_secret: None,
            },
        ))
    }

    /// Generate a new seed for the group. This "refreshes" the group's entropy and should be
    /// called frequently if no other group operations took place for a while.
    pub fn update(
        y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        rng: &Rng,
    ) -> DcgkaOperationResult<ID, OP, PKI, DGM, KMG> {
        let control_message = ControlMessage::Update;

        let recipient_ids: Vec<ID> = Self::member_view(&y, &y.my_id)?
            .into_iter()
            .filter(|member| member != &y.my_id)
            .collect();

        let (y_i, direct_messages) = Self::generate_seed(y, &recipient_ids, rng)?;

        Ok((
            y_i,
            OperationOutput {
                control_message,
                direct_messages,
                me_update_secret: None,
            },
        ))
    }

    /// Called by group members when they receive the "update" control message.
    fn process_update(
        y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        sender: ID,
        seq: OP,
        direct_message: Option<DirectMessage<ID, OP, DGM>>,
        rng: &Rng,
    ) -> DcgkaProcessResult<ID, OP, PKI, DGM, KMG> {
        Self::process_seed(y, &sender, seq, direct_message, rng)
    }

    /// Remove a member from the group.
    ///
    /// This generates a new seed for the remaining members for post-compromise security (PCS).
    pub fn remove(
        y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        removed: ID,
        rng: &Rng,
    ) -> DcgkaOperationResult<ID, OP, PKI, DGM, KMG> {
        let control_message = ControlMessage::Remove { removed };

        let recipient_ids: Vec<ID> = Self::member_view(&y, &y.my_id)?
            .into_iter()
            .filter(|member| member != &y.my_id && member != &removed)
            .collect();

        let (y_i, direct_messages) = Self::generate_seed(y, &recipient_ids, rng)?;

        Ok((
            y_i,
            OperationOutput {
                control_message,
                direct_messages,
                me_update_secret: None,
            },
        ))
    }

    /// Called by group members when they receive the "remove" control message.
    fn process_remove(
        mut y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        sender: ID,
        seq: OP,
        removed: &ID,
        direct_message: Option<DirectMessage<ID, OP, DGM>>,
        rng: &Rng,
    ) -> DcgkaProcessResult<ID, OP, PKI, DGM, KMG> {
        y.dgm = DGM::remove(y.dgm, sender, removed, seq)
            .map_err(|err| DcgkaError::DgmOperation(err))?;
        Self::process_seed(y, &sender, seq, direct_message, rng)
    }

    /// Adds a new group member.
    ///
    /// The added group member will receive a direct "welcome" message, every member will process
    /// an "add" control message.
    pub fn add(
        y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        added: ID,
        rng: &Rng,
    ) -> DcgkaOperationResult<ID, OP, PKI, DGM, KMG> {
        // Construct a control message of type "add" to broadcast to the group
        let control_message = ControlMessage::Add { added };

        // Construct a welcome message that is sent to the new member as a direct message.
        //
        // The welcome message contains the current KDF ratchet state of the sender, encrypted
        // using 2SM, and the history of group membership operations to date (necessary so that the
        // new member can evaluate the DGM function).
        let (y_i, ciphertext) = {
            let chain_secret_bytes = y
                .ratchet
                .get(&y.my_id)
                .ok_or(DcgkaError::MissingRatchetSecret)?
                .as_bytes()
                .to_vec();
            Self::encrypt_to(y, &added, &chain_secret_bytes, rng)?
        };
        let direct_message = DirectMessage {
            recipient: added,
            content: DirectMessageContent::Welcome {
                ciphertext,
                history: {
                    // Send current DGM state to added user. The benefit of doing it like that is
                    // that we can encrypt group operations as well, otherwise we would need to ask
                    // the added user to process all previous (unencrypted) group operations
                    // before.
                    y_i.dgm.clone()
                },
            },
        };

        Ok((
            y_i,
            OperationOutput {
                control_message,
                direct_messages: vec![direct_message],
                me_update_secret: None,
            },
        ))
    }

    /// Called by both the sender and each recipient of an "add" control message, including the new
    /// group member.
    ///
    /// ## Concurrency
    ///
    /// Another scenario that needs to be handled is when two users are concurrently added to the
    /// group. For example, in a group consisting initially of {A, B}, say A adds C to the group,
    /// while concurrently B adds D. User C first processes its own addition and welcome message,
    /// and then processes B's addition of D. However, since C was not a group member at the time B
    /// sent its "add" message, C does not yet have B's ratchet state, so C cannot derive an update
    /// secret for B's "add" message.
    ///
    /// When B finds out about the fact that A has added C, B sends C its ratchet state as usual,
    /// so C can initialize its copy of B's ratchet as before. Similarly, when D finds out about
    /// the fact that A has added C, D sends its ratchet state to C along with the "add-ack"
    /// message. The existing logic therefore handles the concurrent additions: after all acks have
    /// been delivered, C and D have both initialized their copies of all four ratchets, and so
    /// they are able to decrypt application messages that any group member sent after processing
    /// their addition.
    fn process_add(
        mut y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        sender: ID,
        seq: OP,
        added: ID,
        direct_message: Option<DirectMessage<ID, OP, DGM>>,
        rng: &Rng,
    ) -> DcgkaProcessResult<ID, OP, PKI, DGM, KMG> {
        // Local user is the new group member being added. Call "process_welcome" instead and
        // return early.
        if added == y.my_id {
            let Some(DirectMessage {
                recipient,
                content:
                    DirectMessageContent::Welcome {
                        ciphertext,
                        history,
                    },
                ..
            }) = direct_message
            else {
                return match direct_message {
                    Some(direct_message) => Err(DcgkaError::UnexpectedDirectMessageType(
                        DirectMessageType::Welcome,
                        direct_message.message_type(),
                    )),
                    None => Err(DcgkaError::MissingDirectMessage(DirectMessageType::Welcome)),
                };
            };

            if recipient != y.my_id {
                return Err(DcgkaError::NotOurDirectMessage(y.my_id, recipient));
            }

            return Self::process_welcome(y, sender, seq, history, ciphertext);
        }

        // Otherwise extend γ.history with the add operation.
        y.dgm = DGM::add(y.dgm, sender, added, seq).map_err(|err| DcgkaError::DgmOperation(err))?;

        // Determine whether the local user was already a group member at the time the "add"
        // message was sent. This is true in the common case, but may be false if multiple users
        // were added concurrently.
        let is_concurrent = !Self::member_view(&y, &sender)?
            .iter()
            .any(|member| member == &y.my_id);

        let (y_ii, sender_update_secret) = if is_concurrent {
            (y, None)
        } else {
            // We twice update the ratchet for the sender of the "add" message. In both calls to
            // update_ratchet, rather than using a random seed secret, the ratchet input is a
            // constant string ("welcome" and "add" respectively). It is sufficient to use
            // constants here because all existing group members are allowed to know the next
            // update secrets following the add operation.

            // 1. The value returned by the first ratchet update is stored in γ.memberSecret as the
            //    added user's first member secret;
            let (mut y_i, sender_member_secret) =
                Self::update_ratchet(y, &sender, ChainSecret::from_welcome())?;
            y_i.member_secrets.insert(
                (sender, seq, added),
                ChainSecret::from(sender_member_secret),
            );
            // 2. The result of the second ratchet update becomes Isender, the update secret for
            //    the sender of the "add".
            let (y_ii, sender_update_secret) =
                Self::update_ratchet(y_i, &sender, ChainSecret::from_add())?;
            (y_ii, Some(sender_update_secret))
        };

        // If the local user is the sender, we return that update secret.
        if sender == y_ii.my_id {
            return Ok((
                y_ii,
                ProcessOutput {
                    control_message: None,
                    direct_messages: Vec::new(),
                    sender_update_secret,
                    me_update_secret: None,
                },
            ));
        }

        // Otherwise, we need to acknowledge the "add" message, so we construct a control message
        // of type "add-ack" to broadcast (note that add has its own acknowledgment type, whereas
        // create, update and remove all use "ack").
        let control = ControlMessage::AddAck {
            ack_sender: sender,
            ack_seq: seq,
        };

        // We then use 2SM to encrypt our current ratchet state to send as a direct message to the
        // added user, so that they can decrypt subsequent messages we send.
        let (y_iii, ciphertext) = {
            let chain_secret_bytes = y_ii
                .ratchet
                .get(&y_ii.my_id)
                .ok_or(DcgkaError::MissingRatchetSecret)?
                .as_bytes()
                .to_vec();
            Self::encrypt_to(y_ii, &added, &chain_secret_bytes, rng)?
        };
        let forward = DirectMessage {
            recipient: added,
            content: DirectMessageContent::Forward { ciphertext },
        };

        // Finally, we call process_add_ack to compute the local user's update secret Ime, and
        // return it with Isender.
        let (y_iv, output) = {
            let my_id = y_iii.my_id;
            Self::process_add_ack(y_iii, my_id, (&sender, seq), None)?
        };
        let me_update_secret = output.sender_update_secret;

        Ok((
            y_iv,
            ProcessOutput {
                control_message: Some(control),
                direct_messages: vec![forward],
                sender_update_secret,
                me_update_secret,
            },
        ))
    }

    /// Called by both the sender and each recipient of an "add-ack" message, including the new
    /// group member.
    fn process_add_ack(
        mut y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        sender: ID,
        ack: (&ID, OP),
        direct_message: Option<DirectMessage<ID, OP, DGM>>,
    ) -> DcgkaProcessResult<ID, OP, PKI, DGM, KMG> {
        // Add the acknowledgment to γ.history, like in process_ack.
        y.dgm = DGM::ack(y.dgm, sender, ack.1).map_err(|err| DcgkaError::DgmOperation(err))?;

        // If the current user is the new group member, the "add_ack" message is accompanied by the
        // direct message that we constructed in process_add; this direct message dmsg contains the
        // encrypted ratchet state of the sender of the "add_ack", so we decrypt it.
        let y_i = if let Some(direct_message) = direct_message {
            if let DirectMessage {
                recipient,
                content: DirectMessageContent::Forward { ciphertext },
                ..
            } = direct_message
            {
                if recipient != y.my_id {
                    return Err(DcgkaError::NotOurDirectMessage(y.my_id, recipient));
                }

                let (mut y_i, plaintext) = Self::decrypt_from(y, &sender, ciphertext)?;
                let chain_secret = ChainSecret::try_from_bytes(&plaintext)?;
                y_i.ratchet.insert(sender, chain_secret);
                y_i
            } else {
                return Err(DcgkaError::UnexpectedDirectMessageType(
                    DirectMessageType::Forward,
                    direct_message.message_type(),
                ));
            }
        } else {
            y
        };

        // Check if the local user was already a group member at the time the "add_ack" was sent
        // (which may not be the case when there are concurrent additions).
        let is_concurrent = !Self::member_view(&y_i, &sender)?
            .iter()
            .any(|member| member == &y_i.my_id);

        if !is_concurrent {
            // If so, we compute a new update secret I for the sender of the "add_ack" by calling
            // update_ratchet with the constant string "add". In the case of the new member, the
            // ratchet state was just previously initialized. This ratchet update allows all group
            // members, including the new one, to derive each member's update secret for the add
            // operation, but it prevents the new group member from obtaining any update secret
            // from before they were added.
            let (y_ii, sender_update_secret) =
                Self::update_ratchet(y_i, &sender, ChainSecret::from_add())?;
            return Ok((
                y_ii,
                ProcessOutput {
                    control_message: None,
                    direct_messages: Vec::new(),
                    sender_update_secret: Some(sender_update_secret),
                    me_update_secret: None,
                },
            ));
        }

        Ok((y_i, ProcessOutput::default()))
    }

    /// Second function called by a newly added group member (the first is the call to init that
    /// sets up their state).
    fn process_welcome(
        mut y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        sender: ID,
        seq: OP,
        history: DGM::State,
        ciphertext: TwoPartyMessage,
    ) -> DcgkaProcessResult<ID, OP, PKI, DGM, KMG> {
        // Adding user's copy of γ.history sent in their welcome message, which is used to
        // initialize the added user's history.
        //
        // Note that we might receive multiple welcome messages when peers added us concurrently. A
        // DGM implementation might want to account for this.
        y.dgm = DGM::from_welcome(y.dgm, history).map_err(|err| DcgkaError::DgmOperation(err))?;

        // Add ourselves.
        y.dgm =
            DGM::add(y.dgm, sender, y.my_id, seq).map_err(|err| DcgkaError::DgmOperation(err))?;

        // Ciphertext of the adding user's ratchet state, which we decrypt.
        let y_i = {
            let (mut y_i, plaintext) = Self::decrypt_from(y, &sender, ciphertext)?;
            let chain_secret = ChainSecret::try_from_bytes(&plaintext)?;
            y_i.ratchet.insert(sender, chain_secret);
            y_i
        };

        // After γ.ratchet[sender] is initialized, we can call update_ratchet twice with the
        // constant strings "welcome" and "add": exactly the same ratchet operations as every other
        // group member performs in process_add.
        //
        // As before, the result of the first update_ratchet call becomes the first member secret
        // for the added user, and the second returns Isender, the update secret for the sender of
        // the add operation.
        let y_ii = {
            let (mut y_ii, member_secret) =
                Self::update_ratchet(y_i, &sender, ChainSecret::from_welcome())?;
            y_ii.member_secrets
                .insert((sender, seq, y_ii.my_id), ChainSecret::from(member_secret));
            y_ii
        };

        let (y_iii, sender_update_secret) =
            Self::update_ratchet(y_ii, &sender, ChainSecret::from_add())?;

        // Finally, the new group member constructs an "ack" control message (not "add_ack") to
        // broadcast and calls process_ack to compute their first update secret Ime.
        let control = ControlMessage::Ack {
            ack_sender: sender,
            ack_seq: seq,
        };

        // process_ack works as described previously, reading from γ.memberSecret the member secret
        // we just generated, and passing it to update_ratchet.
        //
        // The previous ratchet state for the new member is the empty string ε, as set up by init,
        // so this step initializes the new member's ratchet. Every other group member, on
        // receiving the new member's "ack", will initialize their copy of the new member's ratchet
        // in the same way. By the end of process_welcome, the new group member has obtained update
        // secrets for themselves and the user who added them. They then use those secrets to
        // initialize the ratchets for application messages, allowing them to send messages and
        // decrypt messages from the user who added them. The ratchets for other group members are
        // initialized by process_add_ack.
        let (y_iv, output) = {
            let my_id = y_iii.my_id;
            Self::process_ack(y_iii, my_id, (&sender, seq), None)?
        };
        let me_update_secret = output.sender_update_secret;

        Ok((
            y_iv,
            ProcessOutput {
                control_message: Some(control),
                direct_messages: Vec::new(),
                sender_update_secret: Some(sender_update_secret),
                me_update_secret: Some(
                    me_update_secret.expect("sender update secret from process_ack"),
                ),
            },
        ))
    }

    /// Generates a seed secret using a secure source of random bits, then calls `encrypt_to` to
    /// encrypt it for each other group member using the 2SM protocol. It returns the updated
    /// protocol state and the set of direct messages to send.
    fn generate_seed(
        mut y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        recipients: &[ID],
        rng: &Rng,
    ) -> GenerateSeedResult<ID, OP, PKI, DGM, KMG> {
        let mut direct_messages: Vec<DirectMessage<ID, OP, DGM>> =
            Vec::with_capacity(recipients.len());

        // Generate next seed.
        let next_seed_bytes = rng.random_array()?;
        y.next_seed = Some(NextSeed::from_bytes(next_seed_bytes));

        let y_i = {
            let mut y_loop = y;
            for recipient in recipients {
                // Skip ourselves.
                if recipient == &y_loop.my_id {
                    continue;
                }

                // Encrypt to every recipient.
                let (y_next, ciphertext) =
                    Self::encrypt_to(y_loop, recipient, &next_seed_bytes, rng)?;
                y_loop = y_next;

                direct_messages.push(DirectMessage {
                    recipient: *recipient,
                    content: DirectMessageContent::TwoParty { ciphertext },
                });
            }
            y_loop
        };

        Ok((y_i, direct_messages))
    }

    /// Handle the next seed for the group, either generated locally by us or received as an
    /// encrypted message by another member.
    ///
    /// We use the seed to derive independent member secrets for each group member by combining
    /// the seed secret and each user ID using HKDF.
    fn process_seed(
        mut y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        sender: &ID,
        seq: OP,
        direct_message: Option<DirectMessage<ID, OP, DGM>>,
        rng: &Rng,
    ) -> DcgkaProcessResult<ID, OP, PKI, DGM, KMG> {
        // Determine the set of users who were group members at the time the control message was
        // sent, and hence the set of recipients of the message.
        let recipients: Vec<ID> = Self::member_view(&y, sender)?
            .into_iter()
            .filter(|member| member != sender)
            .collect();

        // Attempt to obtain the seed secret.
        let (mut y_i, next_seed) = if sender == &y.my_id {
            // 1. If the control message was sent by the local user, the last call to generate_seed
            //    placed the seed secret in γ.nextSeed, so we read that variable and then delete
            //    its contents.
            let next_seed = y.next_seed.take().expect("seed was generated before"); // FS
            (y, next_seed)
        } else if recipients.iter().any(|member| member == &y.my_id) {
            // 2. If the control message was sent by another user, and the local user is one of its
            //    recipients, we use decrypt_from to decrypt the direct message containing the seed
            //    secret.
            let Some(DirectMessage {
                recipient,
                content: DirectMessageContent::TwoParty { ciphertext },
                ..
            }) = direct_message
            else {
                return match direct_message {
                    Some(direct_message) => Err(DcgkaError::UnexpectedDirectMessageType(
                        DirectMessageType::TwoParty,
                        direct_message.message_type(),
                    )),
                    None => Err(DcgkaError::MissingDirectMessage(
                        DirectMessageType::TwoParty,
                    )),
                };
            };

            if recipient != y.my_id {
                return Err(DcgkaError::NotOurDirectMessage(y.my_id, recipient));
            }

            let (y_i, plaintext) = Self::decrypt_from(y, sender, ciphertext)?;
            (y_i, NextSeed::try_from_bytes(&plaintext)?)
        } else {
            // 3. Otherwise we return an "ack" message without deriving an update secret. Case 3
            //    may occur when a group member is added concurrently to other messages.
            let control = ControlMessage::Ack {
                ack_sender: *sender,
                ack_seq: seq,
            };
            return Ok((
                y,
                ProcessOutput {
                    control_message: Some(control),
                    direct_messages: Vec::new(),
                    sender_update_secret: None,
                    me_update_secret: None,
                },
            ));
        };

        // Derive independent member secrets for each group member from the seed secret by
        // combining the seed secret and each user ID using HKDF. The secret for the sender of the
        // message is stored in senderSecret, and those for the other group members are stored in
        // γ.memberSecret; the latter are used when we receive acknowledgments from those users.
        for recipient in &recipients {
            let recipient_identity_key = PKI::identity_key(&y_i.pki, recipient)
                .map_err(|err| DcgkaError::IdentityRegistry(err))?
                .ok_or(DcgkaError::MissingIdentityKey(*recipient))?;
            let recipient_member_secret: [u8; RATCHET_KEY_SIZE] = hkdf(
                b"update",
                &{
                    let mut ikm = Vec::with_capacity(RATCHET_KEY_SIZE * 2);
                    ikm.extend_from_slice(next_seed.as_bytes());
                    ikm.extend_from_slice(recipient_identity_key.as_bytes());
                    ikm
                },
                None,
            )?;
            y_i.member_secrets.insert(
                (*sender, seq, *recipient),
                ChainSecret::from_bytes(recipient_member_secret),
            );
        }

        // The sender's member secret is used immediately to update their KDF ratchet and compute
        // their update secret Isender, using update_ratchet.
        let (y_ii, sender_update_secret) = {
            let sender_identity_key = PKI::identity_key(&y_i.pki, sender)
                .map_err(|err| DcgkaError::IdentityRegistry(err))?
                .ok_or(DcgkaError::MissingIdentityKey(*sender))?;
            let sender_member_secret: [u8; RATCHET_KEY_SIZE] = hkdf(
                b"update",
                &{
                    let mut ikm = Vec::with_capacity(RATCHET_KEY_SIZE * 2);
                    ikm.extend_from_slice(next_seed.as_bytes());
                    ikm.extend_from_slice(sender_identity_key.as_bytes());
                    ikm
                },
                None,
            )?;
            Self::update_ratchet(y_i, sender, ChainSecret::from_bytes(sender_member_secret))?
        };

        // We only store the member secrets, and not the seed secret, so that if the user's private
        // state is compromised, the adversary obtains only those member secrets that have not yet
        // been used.
        drop(next_seed); // FS for "next_seed".

        // If the local user is the sender of the control message, we are now finished and return
        // the update secret.
        if sender == &y_ii.my_id {
            return Ok((
                y_ii,
                ProcessOutput {
                    control_message: None,
                    direct_messages: Vec::new(),
                    sender_update_secret: Some(sender_update_secret),
                    me_update_secret: None,
                },
            ));
        }

        // If we received the seed secret from another user, we construct an "ack" control message
        // to broadcast, including the sender ID and sequence number of the message we are
        // acknowledging.
        let control = ControlMessage::Ack {
            ack_sender: *sender,
            ack_seq: seq,
        };

        // Care is required when an add operation occurs concurrently with an update, remove, or
        // another add operation. We want all intended recipients to learn every update secret,
        // since otherwise some users would not be able to decrypt some messages, despite being a
        // group member.
        //
        // For example, consider a group with members {A, B, C}, and say A performs an update while
        // concurrently C adds D to the group. When A distributes a new seed secret through
        // 2SM-encrypted direct messages, D will not be a recipient of one of those direct
        // messages, since A did not know about D's addition at the time of sending. D cannot
        // derive any of the member secrets for this update. When B updates its KDF ratchet using
        // A's seed secret, it will compute an update secret that D does not know, and D will not
        // be able to decrypt B's subsequent application messages.
        //
        // In this example, B may receive the add and the update in either order. If B processes
        // A's update first, the seed secret from A is already incorporated into B's ratchet state
        // at the time of adding D; since B sends this ratchet state to D along with its "add-ack"
        // message, no further action is needed. On the other hand, if B processes the addition of
        // D first, then when B subsequently processes A's update, B must take the member secret it
        // derives from A's seed secret and forward it to D, so that D can compute B's update
        // secret for A's update.
        //
        // Recall that first we set recipients to be the set of group members at the time the
        // update/remove was sent, except for the sender. We then compute the current set of
        // members according to the local node. The set difference thus computes the set of users
        // whose additions have been processed by the local user, but who were not yet known to
        // the sender of the update.
        //
        // If there are any such users, we construct a direct message to each of them. One of the
        // member secrets we computed before is the member secret for the local user. We
        // 2SM-encrypt that member secret for each of the users who need it. This set forward is
        // sent as direct messages along with the "ack".
        let (y_iii, forward_messages) = {
            let members: Vec<ID> = Self::member_view(&y_ii, &y_ii.my_id)?
                .into_iter()
                .filter(|member| member != sender && !recipients.contains(member))
                .collect();
            let mut forward_messages = Vec::with_capacity(members.len());

            let mut y_loop = y_ii;
            for member in members {
                let member_secret_bytes = y_loop
                    .member_secrets
                    .get(&(*sender, seq, y_loop.my_id))
                    .ok_or(DcgkaError::MissingMemberSecret(*sender, seq))?
                    .as_bytes()
                    .to_vec();
                let (y_next, ciphertext) =
                    Self::encrypt_to(y_loop, &member, &member_secret_bytes, rng)?;
                y_loop = y_next;
                forward_messages.push(DirectMessage {
                    recipient: member,
                    content: DirectMessageContent::Forward { ciphertext },
                });
            }

            (y_loop, forward_messages)
        };

        // Compute an update secret Ime for the local user.
        let (y_iv, output) = {
            let my_id = y_iii.my_id;
            Self::process_ack(y_iii, my_id, (sender, seq), None)?
        };
        let me_update_secret = output.sender_update_secret;

        Ok((
            y_iv,
            ProcessOutput {
                control_message: Some(control),
                direct_messages: forward_messages,
                sender_update_secret: Some(sender_update_secret),
                me_update_secret,
            },
        ))
    }

    /// Uses 2SM to encrypt a direct message for another group member. The first time a message is
    /// encrypted to a particular recipient ID, the 2SM protocol state is initialized and stored in
    /// γ.2sm[ID]. We then use 2SM-Send to encrypt the message, and store the updated protocol
    /// state in γ.
    fn encrypt_to(
        mut y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        recipient: &ID,
        plaintext: &[u8],
        rng: &Rng,
    ) -> DcgkaResult<ID, OP, PKI, DGM, KMG, TwoPartyMessage> {
        let y_2sm = match y.two_party.remove(recipient) {
            Some(y_2sm) => y_2sm,
            None => {
                let (pki_i, prekey_bundle) = PKI::key_bundle(y.pki, recipient)
                    .map_err(|err| DcgkaError::PreKeyRegistry(err))?;
                y.pki = pki_i;
                let prekey_bundle = prekey_bundle.ok_or(DcgkaError::MissingPreKeys(*recipient))?;
                TwoParty::<KMG, OneTimeKeyBundle>::init(prekey_bundle)
            }
        };
        let (y_2sm_i, ciphertext) =
            TwoParty::<KMG, OneTimeKeyBundle>::send(y_2sm, &y.my_keys, plaintext, rng)?;
        y.two_party.insert(*recipient, y_2sm_i);
        Ok((y, ciphertext))
    }

    /// Is the reverse of encrypt_to. It similarly initializes the protocol state on first use, and
    /// then uses 2SM-Receive to decrypt the ciphertext, with the protocol state stored in
    /// γ.2sm[ID].
    fn decrypt_from(
        mut y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        sender: &ID,
        ciphertext: TwoPartyMessage,
    ) -> DcgkaResult<ID, OP, PKI, DGM, KMG, Vec<u8>> {
        let y_2sm = match y.two_party.remove(sender) {
            Some(y_2sm) => y_2sm,
            None => {
                let (pki_i, prekey_bundle) = PKI::key_bundle(y.pki, sender)
                    .map_err(|err| DcgkaError::PreKeyRegistry(err))?;
                y.pki = pki_i;
                let prekey_bundle = prekey_bundle.ok_or(DcgkaError::MissingPreKeys(*sender))?;
                TwoParty::<KMG, OneTimeKeyBundle>::init(prekey_bundle)
            }
        };
        let (y_2sm_i, y_my_keys_i, plaintext) =
            TwoParty::<KMG, OneTimeKeyBundle>::receive(y_2sm, y.my_keys, ciphertext)?;
        y.my_keys = y_my_keys_i;
        y.two_party.insert(*sender, y_2sm_i);
        Ok((y, plaintext))
    }

    /// Generates the next update secret for group member ID. It implements the outer KDF of the
    /// ratchet. The ratchet state is stored in γ.ratchet[ID]; we use an HMAC-based key derivation
    /// function HKDF to combine the ratchet state with an input, producing an update secret and a
    /// new ratchet state.
    fn update_ratchet(
        mut y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        member: &ID,
        member_secret: ChainSecret,
    ) -> DcgkaResult<ID, OP, PKI, DGM, KMG, UpdateSecret> {
        let identity_key = PKI::identity_key(&y.pki, member)
            .map_err(|err| DcgkaError::IdentityRegistry(err))?
            .ok_or(DcgkaError::MissingIdentityKey(*member))?;
        // Mix in the previous "outer" ratchet chain secret if it exists.
        let previous_outer_ratchet_key = y.ratchet.get(member);
        let update_secret: [u8; RATCHET_KEY_SIZE] = hkdf(
            b"update",
            &{
                let mut ikm = Vec::with_capacity(RATCHET_KEY_SIZE * 3);
                if let Some(previous_outer_ratchet_key) = previous_outer_ratchet_key {
                    ikm.extend_from_slice(previous_outer_ratchet_key.as_bytes());
                }
                ikm.extend_from_slice(member_secret.as_bytes());
                ikm.extend_from_slice(identity_key.as_bytes());
                ikm
            },
            None,
        )?;
        let next_outer_ratchet_key: [u8; RATCHET_KEY_SIZE] = hkdf(
            b"chain",
            &{
                let mut ikm = Vec::with_capacity(RATCHET_KEY_SIZE * 3);
                if let Some(previous_outer_ratchet_key) = previous_outer_ratchet_key {
                    ikm.extend_from_slice(previous_outer_ratchet_key.as_bytes());
                }
                ikm.extend_from_slice(member_secret.as_bytes());
                ikm.extend_from_slice(identity_key.as_bytes());
                ikm
            },
            None,
        )?;
        drop(member_secret); // FS for `member_secret`
        y.ratchet
            .insert(*member, ChainSecret::from_bytes(next_outer_ratchet_key));
        Ok((y, UpdateSecret::from_bytes(update_secret)))
    }

    /// Computes the set of group members at the time of the most recent control message sent by
    /// user ID. It works by filtering the set of group membership operations to contain only those
    /// seen by ID, and then invoking the Decentralised Group Membership function DGM to compute
    /// the group membership.
    pub fn member_view(
        y: &DcgkaState<ID, OP, PKI, DGM, KMG>,
        viewer: &ID,
    ) -> Result<HashSet<ID>, DcgkaError<ID, OP, PKI, DGM, KMG>> {
        let members =
            DGM::members_view(&y.dgm, viewer).map_err(|err| DcgkaError::MembersView(err))?;
        Ok(members)
    }
}

pub type GenerateSeedResult<ID, OP, PKI, DGM, KMG> = Result<
    (
        DcgkaState<ID, OP, PKI, DGM, KMG>,
        Vec<DirectMessage<ID, OP, DGM>>,
    ),
    DcgkaError<ID, OP, PKI, DGM, KMG>,
>;

pub type DcgkaResult<ID, OP, PKI, DGM, KMG, T> =
    Result<(DcgkaState<ID, OP, PKI, DGM, KMG>, T), DcgkaError<ID, OP, PKI, DGM, KMG>>;

pub type DcgkaProcessResult<ID, OP, PKI, DGM, KMG> =
    DcgkaResult<ID, OP, PKI, DGM, KMG, ProcessOutput<ID, OP, DGM>>;

pub type DcgkaOperationResult<ID, OP, PKI, DGM, KMG> =
    DcgkaResult<ID, OP, PKI, DGM, KMG, OperationOutput<ID, OP, DGM>>;

/// Message that should be broadcast to the group.
///
/// The control message must be distributed to the other group members through Authenticated Causal
/// Broadcast, calling the process function on the recipient when they are delivered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlMessage<ID, OP> {
    Create { initial_members: Vec<ID> },
    Ack { ack_sender: ID, ack_seq: OP },
    Update,
    Remove { removed: ID },
    Add { added: ID },
    AddAck { ack_sender: ID, ack_seq: OP },
}

impl<ID, OP> Display for ControlMessage<ID, OP> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ControlMessage::Create { .. } => "create",
                ControlMessage::Ack { .. } => "ack",
                ControlMessage::Update => "update",
                ControlMessage::Remove { .. } => "remove",
                ControlMessage::Add { .. } => "add",
                ControlMessage::AddAck { .. } => "add_ack",
            }
        )
    }
}

/// Arguments required to process a group operation received from another member.
#[derive(Clone, Debug)]
pub struct ProcessInput<ID, OP, DGM>
where
    DGM: AckedGroupMembership<ID, OP>,
{
    /// Sequence number, which consecutively numbers successive control messages from the same
    /// sender.
    pub seq: OP,

    /// Author of this message.
    pub sender: ID,

    /// Message received from this author.
    pub control_message: ControlMessage<ID, OP>,

    /// Optional direct message for us.
    ///
    /// Applications need to filter the direct message for the correct recipient before passing it
    /// as an input. There can always only be max. 1 direct message per recipient.
    pub direct_message: Option<DirectMessage<ID, OP, DGM>>,
}

/// Calling "process" returns a 4-tuple `(control, dmsgs, Is, Ir)`, where `Is` is an update secret
/// for the sender of the message being processed, `Ir` is an update secret for the recipient.
#[derive(Debug)]
pub struct ProcessOutput<ID, OP, DGM>
where
    DGM: AckedGroupMembership<ID, OP>,
{
    /// Control message that should be broadcast to the group.
    pub control_message: Option<ControlMessage<ID, OP>>,

    /// Direct messages to be sent to specific members.
    pub direct_messages: Vec<DirectMessage<ID, OP, DGM>>,

    /// Our next update key for the message ratchet for incoming messages from the sender.
    pub sender_update_secret: Option<UpdateSecret>,

    /// Our next update key for the message ratchet for outgoing messages to the group.
    pub me_update_secret: Option<UpdateSecret>,
}

impl<ID, OP, DGM> Default for ProcessOutput<ID, OP, DGM>
where
    DGM: AckedGroupMembership<ID, OP>,
{
    fn default() -> Self {
        Self {
            control_message: None,
            direct_messages: Vec::new(),
            sender_update_secret: None,
            me_update_secret: None,
        }
    }
}

/// Calling "create", "add", "remove" and "update" return a tuple of three variables (`control`,
/// `dmsgs` and `I`) after changing the state for the current user.
///
/// `control` is a control message that should be broadcast to the group, `dmsgs` is a set of `(u,
/// m)` pairs where `m` is a direct message that should be sent to user `u`, and `I` is a new
/// update secret for the current user.
pub struct OperationOutput<ID, OP, DGM>
where
    DGM: AckedGroupMembership<ID, OP>,
{
    /// Control message that should be broadcast to the group.
    pub control_message: ControlMessage<ID, OP>,

    /// Set of messages directly to be sent to specific users.
    pub direct_messages: Vec<DirectMessage<ID, OP, DGM>>,

    /// Our next update key for the message ratchet for outgoing messages to the group.
    pub me_update_secret: Option<UpdateSecret>,
}

/// Direct message that should be sent to a single member.
///
/// The direct message must be distributed to the other group members through Authenticated Causal
/// Broadcast, calling the process function on the recipient when they are delivered.
///
/// If direct messages are sent along with a control message, we assume that the direct message for
/// the appropriate recipient is delivered in the same call to process. Our algorithm never sends a
/// direct message without an associated broadcast control message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectMessage<ID, OP, DGM>
where
    DGM: AckedGroupMembership<ID, OP>,
{
    pub recipient: ID,
    pub content: DirectMessageContent<ID, OP, DGM>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DirectMessageContent<ID, OP, DGM>
where
    DGM: AckedGroupMembership<ID, OP>,
{
    Welcome {
        ciphertext: TwoPartyMessage,
        history: DGM::State,
    },
    TwoParty {
        ciphertext: TwoPartyMessage,
    },
    Forward {
        ciphertext: TwoPartyMessage,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectMessageType {
    Welcome,
    TwoParty,
    Forward,
}

impl Display for DirectMessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                DirectMessageType::Welcome => "welcome",
                DirectMessageType::TwoParty => "2sm",
                DirectMessageType::Forward => "forward",
            }
        )
    }
}

impl<ID, OP, DGM> DirectMessage<ID, OP, DGM>
where
    DGM: AckedGroupMembership<ID, OP>,
{
    pub fn message_type(&self) -> DirectMessageType {
        match self.content {
            DirectMessageContent::Welcome { .. } => DirectMessageType::Welcome,
            DirectMessageContent::TwoParty { .. } => DirectMessageType::TwoParty,
            DirectMessageContent::Forward { .. } => DirectMessageType::Forward,
        }
    }
}

/// Randomly generated seed we keep temporarily around when creating or updating a group or
/// removing a member.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct NextSeed(Secret<RATCHET_KEY_SIZE>);

impl NextSeed {
    pub fn from_bytes(bytes: [u8; RATCHET_KEY_SIZE]) -> Self {
        Self(Secret::from_bytes(bytes))
    }

    pub fn try_from_bytes<ID, OP, PKI, DGM, KMG>(
        bytes: &[u8],
    ) -> Result<Self, DcgkaError<ID, OP, PKI, DGM, KMG>>
    where
        PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
        DGM: AckedGroupMembership<ID, OP>,
        KMG: PreKeyManager,
    {
        let bytes: [u8; RATCHET_KEY_SIZE] =
            bytes.try_into().map_err(|_| DcgkaError::InvalidKeySize)?;
        Ok(Self::from_bytes(bytes))
    }

    pub(crate) fn as_bytes(&self) -> &[u8; RATCHET_KEY_SIZE] {
        self.0.as_bytes()
    }
}

/// Chain secret for the "outer" key-agreement ratchet.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct ChainSecret(Secret<RATCHET_KEY_SIZE>);

impl ChainSecret {
    pub fn from_welcome() -> Self {
        // BLAKE3 hash of the byte-representation of the utf8 string "welcome".
        Self::from_bytes([
            168, 234, 241, 118, 147, 12, 137, 47, 48, 26, 61, 243, 183, 11, 158, 143, 99, 219, 142,
            131, 41, 18, 245, 167, 132, 195, 241, 26, 89, 106, 154, 134,
        ])
    }

    pub fn from_add() -> Self {
        // BLAKE3 hash of the byte-representation of the utf8 string "add".
        Self::from_bytes([
            58, 3, 204, 193, 45, 117, 68, 208, 41, 238, 11, 13, 169, 250, 180, 215, 22, 4, 43, 226,
            179, 34, 182, 188, 85, 49, 221, 39, 150, 98, 220, 156,
        ])
    }

    pub fn from_bytes(bytes: [u8; RATCHET_KEY_SIZE]) -> Self {
        Self(Secret::from_bytes(bytes))
    }

    pub fn try_from_bytes<ID, OP, PKI, DGM, KEY>(
        bytes: &[u8],
    ) -> Result<Self, DcgkaError<ID, OP, PKI, DGM, KEY>>
    where
        PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
        DGM: AckedGroupMembership<ID, OP>,
        KEY: PreKeyManager,
    {
        let bytes: [u8; RATCHET_KEY_SIZE] =
            bytes.try_into().map_err(|_| DcgkaError::InvalidKeySize)?;
        Ok(Self::from_bytes(bytes))
    }

    pub(crate) fn as_bytes(&self) -> &[u8; RATCHET_KEY_SIZE] {
        self.0.as_bytes()
    }
}

impl From<UpdateSecret> for ChainSecret {
    fn from(value: UpdateSecret) -> Self {
        Self(value.0)
    }
}

/// Chain secret for the "inner" message ratchet.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct UpdateSecret(Secret<RATCHET_KEY_SIZE>);

impl UpdateSecret {
    pub fn from_bytes(bytes: [u8; RATCHET_KEY_SIZE]) -> Self {
        Self(Secret::from_bytes(bytes))
    }

    pub fn try_from_bytes<ID, OP, PKI, DGM, KMG>(
        bytes: &[u8],
    ) -> Result<Self, DcgkaError<ID, OP, PKI, DGM, KMG>>
    where
        PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
        DGM: AckedGroupMembership<ID, OP>,
        KMG: PreKeyManager,
    {
        let bytes: [u8; RATCHET_KEY_SIZE] =
            bytes.try_into().map_err(|_| DcgkaError::InvalidKeySize)?;
        Ok(Self::from_bytes(bytes))
    }

    pub(crate) fn as_bytes(&self) -> &[u8; RATCHET_KEY_SIZE] {
        self.0.as_bytes()
    }
}

impl From<UpdateSecret> for Secret<RATCHET_KEY_SIZE> {
    fn from(update_secret: UpdateSecret) -> Self {
        update_secret.0
    }
}

#[derive(Debug, Error)]
pub enum DcgkaError<ID, OP, PKI, DGM, KMG>
where
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
    DGM: AckedGroupMembership<ID, OP>,
    KMG: PreKeyManager,
{
    #[error("the given key does not match the required 32 byte length")]
    InvalidKeySize,

    #[error("expected ratchet secret but couldn't find anything")]
    MissingRatchetSecret,

    #[error("expected message secret for {0} at seq {1} but couldn't find anything")]
    MissingMemberSecret(ID, OP),

    #[error("expected direct message of type \"{0}\" but got nothing instead")]
    MissingDirectMessage(DirectMessageType),

    #[error("expected direct message of type \"{0}\" but got message of type \"{1}\" instead")]
    UnexpectedDirectMessageType(DirectMessageType, DirectMessageType),

    #[error("direct message recipient mismatch, expected recipient: {1}, actual recipient: {0}")]
    NotOurDirectMessage(ID, ID),

    #[error("computing members view from dgm failed: {0}")]
    MembersView(DGM::Error),

    #[error("dgm operation failed: {0}")]
    DgmOperation(DGM::Error),

    #[error("failed retrieving bundle from pre key registry: {0}")]
    PreKeyRegistry(<PKI as PreKeyRegistry<ID, OneTimeKeyBundle>>::Error),

    #[error("failed retrieving identity from registry: {0}")]
    IdentityRegistry(<PKI as IdentityRegistry<ID, PKI::State>>::Error),

    #[error("missing key bundle for member {0}")]
    MissingPreKeys(ID),

    #[error("missing identity key for member {0}")]
    MissingIdentityKey(ID),

    #[error(transparent)]
    Rng(#[from] RngError),

    #[error(transparent)]
    KeyManager(KMG::Error),

    #[error(transparent)]
    TwoParty(#[from] TwoPartyError),

    #[error(transparent)]
    Hdkf(#[from] HkdfError),
}
