// SPDX-License-Identifier: MIT OR Apache-2.0

//! A decentralized continuous group key agreement protocol (DCGKA) for p2panda's "data encryption"
//! scheme with forward secrecy and post-compromise security.
//!
//! This uses the 2SM (Two-Party Secure Messaging) key agreement scheme internally with strong
//! forward secrecy guarantees.
use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display};
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::Rng;
use crate::data_scheme::{GroupSecret, GroupSecretError, SecretBundle, SecretBundleState};
use crate::key_bundle::LongTermKeyBundle;
use crate::traits::{
    GroupMembership, IdentityHandle, IdentityManager, IdentityRegistry, OperationId, PreKeyManager,
    PreKeyRegistry,
};
use crate::two_party::{TwoParty, TwoPartyError, TwoPartyMessage, TwoPartyState};

/// A decentralized continuous group key agreement protocol (DCGKA) for p2panda's "data encryption"
/// scheme with forward secrecy and post-compromise security.
pub struct Dcgka<ID, OP, PKI, DGM, KMG> {
    _marker: PhantomData<(ID, OP, PKI, DGM, KMG)>,
}

/// Serializable state of "data encryption" DCGKA (for persistence).
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct DcgkaState<ID, OP, PKI, DGM, KMG>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, LongTermKeyBundle>,
    DGM: GroupMembership<ID, OP>,
    KMG: IdentityManager<KMG::State> + PreKeyManager,
{
    /// Public Key Infrastructure (PKI). From here we retrieve the identity keys and long-term
    /// pre-key bundles for each member to do 2SM.
    pub pki: PKI::State,

    /// Our own key manager state holding the secret parts for our own identity keys and published
    /// long-term pre-key bundles so we can do 2SM.
    pub my_keys: KMG::State,

    /// Our id which is used as a unique handle inside this group.
    pub my_id: ID,

    /// Handlers for each member to manage the "Two-Party Secure Messaging" (2SM) key-agreement
    /// protocol as specified in the paper.
    pub two_party: HashMap<ID, TwoPartyState<LongTermKeyBundle>>,

    /// Decentralised group membership (DGM) state.
    pub dgm: DGM::State,
}

impl<ID, OP, PKI, DGM, KMG> Dcgka<ID, OP, PKI, DGM, KMG>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, LongTermKeyBundle>,
    DGM: GroupMembership<ID, OP>,
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
            two_party: HashMap::new(),
            dgm,
        }
    }

    /// Handler for when a "remote" control message is received from the network or when we need to
    /// process our "local" operation after calling "create", "update", "add" or "remove".
    ///
    /// It takes the user ID of the message sender, a control message, and a direct message (or
    /// none if there is no associated direct message).
    ///
    /// Control messages are expected to be authenticated and causally ordered.
    pub fn process(
        y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        input: ProcessInput<ID, OP, DGM>,
    ) -> DcgkaProcessResult<ID, OP, PKI, DGM, KMG> {
        let ProcessInput {
            sender,
            control_message,
            direct_message,
            seq,
        } = input;
        let (y_i, output) = match control_message {
            ControlMessage::Create { initial_members } => {
                Self::process_create(y, &sender, initial_members, direct_message)?
            }
            ControlMessage::Update => Self::process_update(y, &sender, direct_message)?,
            ControlMessage::Remove { removed } => {
                Self::process_remove(y, sender, seq, &removed, direct_message)?
            }
            ControlMessage::Add { added } => {
                Self::process_add(y, sender, seq, added, direct_message)?
            }
        };
        Ok((y_i, output))
    }

    /// Takes a set of users IDs (including us), an initial group secret and creates a new group with those members who will learn about this secret.
    ///
    /// Note that every member ID needs to be unique for this group.
    pub fn create(
        y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        initial_members: Vec<ID>,
        group_secret: &GroupSecret,
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
        let (y_ii, direct_messages) =
            Self::send_group_secret(y, &initial_members, group_secret, rng)?;

        Ok((
            y_ii,
            OperationOutput {
                control_message,
                direct_messages,
            },
        ))
    }

    /// Called by group members when they receive the "create" message.
    fn process_create(
        mut y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        sender: &ID,
        initial_members: Vec<ID>,
        direct_message: Option<DirectMessage<ID, OP, DGM>>,
    ) -> DcgkaProcessResult<ID, OP, PKI, DGM, KMG> {
        y.dgm =
            DGM::create(y.my_id, &initial_members).map_err(|err| DcgkaError::DgmOperation(err))?;
        Self::process_secret(y, sender, direct_message)
    }

    /// Establishes a new secret for the group.
    pub fn update(
        y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        group_secret: &GroupSecret,
        rng: &Rng,
    ) -> DcgkaOperationResult<ID, OP, PKI, DGM, KMG> {
        let control_message = ControlMessage::Update;

        let recipient_ids: Vec<ID> = Self::members(&y)?
            .into_iter()
            .filter(|member| member != &y.my_id)
            .collect();

        let (y_i, direct_messages) = Self::send_group_secret(y, &recipient_ids, group_secret, rng)?;

        Ok((
            y_i,
            OperationOutput {
                control_message,
                direct_messages,
            },
        ))
    }

    /// Called by group members when they receive the "update" control message.
    fn process_update(
        y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        sender: &ID,
        direct_message: Option<DirectMessage<ID, OP, DGM>>,
    ) -> DcgkaProcessResult<ID, OP, PKI, DGM, KMG> {
        Self::process_secret(y, sender, direct_message)
    }

    /// Remove a member from the group.
    ///
    /// This takes a new secret as an argument for the remaining members for post-compromise security (PCS).
    pub fn remove(
        y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        removed: ID,
        group_secret: &GroupSecret,
        rng: &Rng,
    ) -> DcgkaOperationResult<ID, OP, PKI, DGM, KMG> {
        let control_message = ControlMessage::Remove { removed };

        let recipient_ids: Vec<ID> = Self::members(&y)?
            .into_iter()
            .filter(|member| member != &y.my_id && member != &removed)
            .collect();

        let (y_i, direct_messages) = Self::send_group_secret(y, &recipient_ids, group_secret, rng)?;

        Ok((
            y_i,
            OperationOutput {
                control_message,
                direct_messages,
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
    ) -> DcgkaProcessResult<ID, OP, PKI, DGM, KMG> {
        y.dgm = DGM::remove(y.dgm, sender, removed, seq)
            .map_err(|err| DcgkaError::DgmOperation(err))?;
        Self::process_secret(y, &sender, direct_message)
    }

    /// Adds a new group member.
    ///
    /// The added group member will receive a direct "welcome" message containing all previously
    /// used secrets of the group in form of a [`SecretBundle`]. Every member will process
    /// an "add" control message.
    pub fn add(
        y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        added: ID,
        bundle: &SecretBundleState,
        rng: &Rng,
    ) -> DcgkaOperationResult<ID, OP, PKI, DGM, KMG> {
        // Construct a control message of type "add" to broadcast to the group.
        let control_message = ControlMessage::Add { added };

        // Construct a welcome message that is sent to the new member as a direct message.
        let (y_i, ciphertext) = {
            let bundle_bytes = bundle.to_bytes()?;
            Self::encrypt_to(y, &added, &bundle_bytes, rng)?
        };
        let direct_message = DirectMessage {
            recipient: added,
            content: DirectMessageContent::Welcome {
                ciphertext,
                history: y_i.dgm.clone(),
            },
        };

        Ok((
            y_i,
            OperationOutput {
                control_message,
                direct_messages: vec![direct_message],
            },
        ))
    }

    /// Called by both the sender and each recipient of an "add" control message, including the new
    /// group member.
    fn process_add(
        mut y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        sender: ID,
        seq: OP,
        added: ID,
        direct_message: Option<DirectMessage<ID, OP, DGM>>,
    ) -> DcgkaProcessResult<ID, OP, PKI, DGM, KMG> {
        y.dgm = DGM::add(y.dgm, sender, added, seq).map_err(|err| DcgkaError::DgmOperation(err))?;

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

            return Self::process_welcome(y, sender, ciphertext, history);
        }

        Ok((y, GroupSecretOutput::None))
    }

    /// Second function called by a newly added group member (the first is the call to init that
    /// sets up their state).
    fn process_welcome(
        mut y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        sender: ID,
        ciphertext: TwoPartyMessage,
        history: DGM::State,
    ) -> DcgkaProcessResult<ID, OP, PKI, DGM, KMG> {
        y.dgm = DGM::from_welcome(y.my_id, history).map_err(|err| DcgkaError::DgmOperation(err))?;

        let (y_i, bundle) = {
            let (y_i, plaintext) = Self::decrypt_from(y, &sender, ciphertext)?;
            let bundle = SecretBundle::try_from_bytes(&plaintext)?;
            (y_i, bundle)
        };

        Ok((y_i, GroupSecretOutput::Bundle(bundle)))
    }

    /// Takes a group secret, then calls `encrypt_to` to
    /// encrypt it for each other group member using the 2SM protocol. It returns the updated
    /// protocol state and the set of direct messages to send.
    fn send_group_secret(
        y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        recipients: &[ID],
        group_secret: &GroupSecret,
        rng: &Rng,
    ) -> SendSecretResult<ID, OP, PKI, DGM, KMG> {
        let mut direct_messages: Vec<DirectMessage<ID, OP, DGM>> =
            Vec::with_capacity(recipients.len());

        let y_i = {
            let mut y_loop = y;
            for recipient in recipients {
                // Skip ourselves.
                if recipient == &y_loop.my_id {
                    continue;
                }

                // Encrypt to every recipient.
                let (y_next, ciphertext) =
                    Self::encrypt_to(y_loop, recipient, &group_secret.to_bytes()?, rng)?;
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

    /// Handles a new secret for the group, received as an
    /// encrypted, direct message from another member.
    fn process_secret(
        y: DcgkaState<ID, OP, PKI, DGM, KMG>,
        sender: &ID,
        direct_message: Option<DirectMessage<ID, OP, DGM>>,
    ) -> DcgkaProcessResult<ID, OP, PKI, DGM, KMG> {
        let Some(direct_message) = direct_message else {
            return Ok((y, GroupSecretOutput::None));
        };

        let DirectMessage {
            recipient,
            content: DirectMessageContent::TwoParty { ciphertext },
            ..
        } = direct_message
        else {
            return Err(DcgkaError::UnexpectedDirectMessageType(
                DirectMessageType::TwoParty,
                direct_message.message_type(),
            ));
        };

        if recipient != y.my_id {
            return Err(DcgkaError::NotOurDirectMessage(y.my_id, recipient));
        }

        let (y_i, plaintext) = Self::decrypt_from(y, sender, ciphertext)?;
        let group_secret = GroupSecret::try_from_bytes(&plaintext)?;

        Ok((y_i, GroupSecretOutput::Secret(group_secret)))
    }

    /// Uses 2SM to encrypt a direct message for another group member. The first time a message is
    /// encrypted to a particular recipient ID, the 2SM protocol state is initialised and stored in
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
                TwoParty::<KMG, LongTermKeyBundle>::init(prekey_bundle)
            }
        };
        let (y_2sm_i, ciphertext) =
            TwoParty::<KMG, LongTermKeyBundle>::send(y_2sm, &y.my_keys, plaintext, rng)?;
        y.two_party.insert(*recipient, y_2sm_i);
        Ok((y, ciphertext))
    }

    /// Is the reverse of encrypt_to. It similarly initialises the protocol state on first use, and
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
                TwoParty::<KMG, LongTermKeyBundle>::init(prekey_bundle)
            }
        };
        let (y_2sm_i, y_my_keys_i, plaintext) =
            TwoParty::<KMG, LongTermKeyBundle>::receive(y_2sm, y.my_keys, ciphertext)?;
        y.my_keys = y_my_keys_i;
        y.two_party.insert(*sender, y_2sm_i);
        Ok((y, plaintext))
    }

    /// Returns the set of group members at the current time.
    pub fn members(
        y: &DcgkaState<ID, OP, PKI, DGM, KMG>,
    ) -> Result<HashSet<ID>, DcgkaError<ID, OP, PKI, DGM, KMG>> {
        let members = DGM::members(&y.dgm).map_err(|err| DcgkaError::GroupMembership(err))?;
        Ok(members)
    }
}

pub type SendSecretResult<ID, OP, PKI, DGM, KMG> = Result<
    (
        DcgkaState<ID, OP, PKI, DGM, KMG>,
        Vec<DirectMessage<ID, OP, DGM>>,
    ),
    DcgkaError<ID, OP, PKI, DGM, KMG>,
>;

pub type DcgkaResult<ID, OP, PKI, DGM, KMG, T> =
    Result<(DcgkaState<ID, OP, PKI, DGM, KMG>, T), DcgkaError<ID, OP, PKI, DGM, KMG>>;

pub type DcgkaProcessResult<ID, OP, PKI, DGM, KMG> =
    DcgkaResult<ID, OP, PKI, DGM, KMG, GroupSecretOutput>;

pub type DcgkaOperationResult<ID, OP, PKI, DGM, KMG> =
    DcgkaResult<ID, OP, PKI, DGM, KMG, OperationOutput<ID, OP, DGM>>;

/// Message that should be broadcast to the group.
///
/// The control message must be distributed to the other group members through Authenticated Causal
/// Broadcast, calling the process function on the recipient when they are delivered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlMessage<ID> {
    Create { initial_members: Vec<ID> },
    Update,
    Remove { removed: ID },
    Add { added: ID },
}

impl<ID> Display for ControlMessage<ID> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ControlMessage::Create { .. } => "create",
                ControlMessage::Update => "update",
                ControlMessage::Remove { .. } => "remove",
                ControlMessage::Add { .. } => "add",
            }
        )
    }
}

/// Arguments required to process a group operation received from another member.
#[derive(Clone, Debug)]
pub struct ProcessInput<ID, OP, DGM>
where
    DGM: GroupMembership<ID, OP>,
{
    /// Sequence number, which consecutively numbers successive control messages from the same
    /// sender.
    pub seq: OP,

    /// Author of this message.
    pub sender: ID,

    /// Message received from this author.
    pub control_message: ControlMessage<ID>,

    /// Optional direct message for us.
    ///
    /// Applications need to filter the direct message for the correct recipient before passing it
    /// as an input. There can always only be max. 1 direct message per recipient.
    pub direct_message: Option<DirectMessage<ID, OP, DGM>>,
}

/// Secret encryption keys we've learned about after processing a member's control message.
#[derive(Debug, PartialEq, Eq)]
pub enum GroupSecretOutput {
    None,
    Secret(GroupSecret),
    Bundle(SecretBundleState),
}

/// Calling "create", "add", "remove" and "update" returns a control message that should be
/// broadcast to the group and a set of direct messages that should be sent to the regarding
/// members.
#[derive(Debug)]
pub struct OperationOutput<ID, OP, DGM>
where
    DGM: GroupMembership<ID, OP>,
{
    /// Control message that should be broadcast to the group.
    pub control_message: ControlMessage<ID>,

    /// Set of messages directly to be sent to specific users.
    pub direct_messages: Vec<DirectMessage<ID, OP, DGM>>,
}

/// Direct message that should be sent to a single member.
///
/// The direct message must be distributed to the other group members through Authenticated Causal
/// Broadcast, calling the process function on the recipient when they are delivered.
///
/// If direct messages are sent along with a control message, we assume that the direct message for
/// the appropriate recipient is delivered in the same call to process. Our algorithm never sends a
/// direct message without an associated broadcast control message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectMessage<ID, OP, DGM>
where
    DGM: GroupMembership<ID, OP>,
{
    pub recipient: ID,
    pub content: DirectMessageContent<ID, OP, DGM>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectMessageType {
    Welcome,
    TwoParty,
}

impl Display for DirectMessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                DirectMessageType::Welcome => "welcome",
                DirectMessageType::TwoParty => "2sm",
            }
        )
    }
}

impl<ID, OP, DGM> DirectMessage<ID, OP, DGM>
where
    DGM: GroupMembership<ID, OP>,
{
    pub fn message_type(&self) -> DirectMessageType {
        match self.content {
            DirectMessageContent::Welcome { .. } => DirectMessageType::Welcome,
            DirectMessageContent::TwoParty { .. } => DirectMessageType::TwoParty,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectMessageContent<ID, OP, DGM>
where
    DGM: GroupMembership<ID, OP>,
{
    Welcome {
        ciphertext: TwoPartyMessage,
        history: DGM::State,
    },
    TwoParty {
        ciphertext: TwoPartyMessage,
    },
}

#[derive(Debug, Error)]
pub enum DcgkaError<ID, OP, PKI, DGM, KMG>
where
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, LongTermKeyBundle>,
    DGM: GroupMembership<ID, OP>,
    KMG: PreKeyManager,
{
    #[error("expected direct message of type \"{0}\" but got nothing instead")]
    MissingDirectMessage(DirectMessageType),

    #[error("expected direct message of type \"{0}\" but got message of type \"{1}\" instead")]
    UnexpectedDirectMessageType(DirectMessageType, DirectMessageType),

    #[error("direct message recipient mismatch, expected recipient: {1}, actual recipient: {0}")]
    NotOurDirectMessage(ID, ID),

    #[error("computing members view from dgm failed: {0}")]
    GroupMembership(DGM::Error),

    #[error("dgm operation failed: {0}")]
    DgmOperation(DGM::Error),

    #[error("failed retrieving bundle from pre key registry: {0}")]
    PreKeyRegistry(<PKI as PreKeyRegistry<ID, LongTermKeyBundle>>::Error),

    #[error("failed retrieving identity from registry: {0}")]
    IdentityRegistry(<PKI as IdentityRegistry<ID, PKI::State>>::Error),

    #[error("missing key bundle for member {0}")]
    MissingPreKeys(ID),

    #[error(transparent)]
    GroupSecret(#[from] GroupSecretError),

    #[error(transparent)]
    KeyManager(KMG::Error),

    #[error(transparent)]
    TwoParty(#[from] TwoPartyError),
}
