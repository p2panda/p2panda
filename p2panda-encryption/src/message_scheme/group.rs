// SPDX-License-Identifier: MIT OR Apache-2.0

//! API to manage groups using the "Message Encryption" scheme and process remote control- and
//! application messages.
#![allow(clippy::type_complexity)]
use std::collections::{HashMap, HashSet};
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::Rng;
use crate::crypto::aead::AeadError;
use crate::key_bundle::OneTimeKeyBundle;
use crate::message_scheme::dcgka::{
    ControlMessage, Dcgka, DcgkaError, DcgkaState, DirectMessage, OperationOutput, ProcessInput,
};
use crate::message_scheme::message::{decrypt_message, encrypt_message};
use crate::message_scheme::ratchet::{
    DecryptionRatchet, DecryptionRatchetState, Generation, RatchetError, RatchetSecret,
    RatchetSecretState,
};
use crate::traits::{
    AckedGroupMembership, ForwardSecureGroupMessage, ForwardSecureMessageContent,
    ForwardSecureOrdering, IdentityHandle, IdentityManager, IdentityRegistry, OperationId,
    PreKeyManager, PreKeyRegistry,
};

/// API to manage groups using the "Message Encryption" scheme with strong security guarantees.
pub struct MessageGroup<ID, OP, PKI, DGM, KMG, ORD> {
    _marker: PhantomData<(ID, OP, PKI, DGM, KMG, ORD)>,
}

/// Group state for "Message Encryption" scheme. Serializable for persistence.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct GroupState<ID, OP, PKI, DGM, KMG, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
    PKI::State: Clone,
    DGM: AckedGroupMembership<ID, OP>,
    KMG: IdentityManager<KMG::State> + PreKeyManager,
    KMG::State: Clone,
    ORD: ForwardSecureOrdering<ID, OP, DGM>,
{
    pub(crate) my_id: ID,
    pub(crate) dcgka: DcgkaState<ID, OP, PKI, DGM, KMG>,
    pub(crate) orderer: ORD::State,
    pub(crate) welcome: Option<ORD::Message>,
    pub(crate) ratchet: Option<RatchetSecretState>,
    pub(crate) decryption_ratchet: HashMap<ID, DecryptionRatchetState>,
    pub(crate) config: GroupConfig,
}

impl<ID, OP, PKI, DGM, KMG, ORD> MessageGroup<ID, OP, PKI, DGM, KMG, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
    PKI::State: Clone,
    DGM: AckedGroupMembership<ID, OP>,
    KMG: IdentityManager<KMG::State> + PreKeyManager,
    KMG::State: Clone,
    ORD: ForwardSecureOrdering<ID, OP, DGM>,
{
    /// Returns initial state for messaging group.
    ///
    /// This needs to be called before creating or being added to a group.
    pub fn init(
        my_id: ID,
        my_keys: KMG::State,
        pki: PKI::State,
        dgm: DGM::State,
        orderer: ORD::State,
        config: GroupConfig,
    ) -> GroupState<ID, OP, PKI, DGM, KMG, ORD> {
        GroupState {
            my_id,
            dcgka: Dcgka::init(my_id, my_keys, pki, dgm),
            orderer,
            welcome: None,
            ratchet: None,
            decryption_ratchet: HashMap::new(),
            config,
        }
    }

    /// Creates new group with initial set of members.
    pub fn create(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        initial_members: Vec<ID>,
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        // If we have an encryption ratchet we already established a group (either by creating or
        // processing a "welcome" message in the past).
        if y.ratchet.is_some() {
            return Err(GroupError::GroupAlreadyEstablished);
        }

        // Create new group with initial members.
        let (y_dcgka_i, pre) = Dcgka::create(y.dcgka, initial_members, rng)?;
        y.dcgka = y_dcgka_i;

        let (mut y_i, message) = Self::process_local(y, pre, rng)?;

        // Set our own "create" as the "welcome" message.
        let y_orderer_i = ORD::set_welcome(y_i.orderer, &message).map_err(GroupError::Orderer)?;
        y_i.orderer = y_orderer_i;

        Ok((y_i, message))
    }

    /// Adds new member to group.
    pub fn add(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        added: ID,
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        if y.ratchet.is_none() {
            return Err(GroupError::GroupNotYetEstablished);
        }

        if y.my_id == added {
            return Err(GroupError::NotAddOurselves);
        }

        if Self::members(&y)?.contains(&added) {
            return Err(GroupError::AddedExistsAlready(added));
        }

        // Add a new member to the group.
        let (y_dcgka_i, pre) = Dcgka::add(y.dcgka, added, rng)?;
        y.dcgka = y_dcgka_i;

        Self::process_local(y, pre, rng)
    }

    /// Removes member from group. It is possible to remove ourselves.
    pub fn remove(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        removed: ID,
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        if y.ratchet.is_none() {
            return Err(GroupError::GroupNotYetEstablished);
        }

        if !Self::members(&y)?.contains(&removed) {
            return Err(GroupError::InexistentRemovedMember(removed));
        }

        // Remove a member from the group.
        let (y_dcgka_i, pre) = Dcgka::remove(y.dcgka, removed, rng)?;
        y.dcgka = y_dcgka_i;

        Self::process_local(y, pre, rng)
    }

    /// Updates group secret and provides all members with fresh entropy.
    pub fn update(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        if y.ratchet.is_none() {
            return Err(GroupError::GroupNotYetEstablished);
        }

        // Update the group by generating a new seed.
        let (y_dcgka_i, pre) = Dcgka::update(y.dcgka, rng)?;
        y.dcgka = y_dcgka_i;

        Self::process_local(y, pre, rng)
    }

    /// Handler for incoming, remote messages.
    ///
    /// This yields a list of "outputs" which can be either control messages which need to be
    /// broadcast to all members in the group or decrypted application message payloads.
    ///
    /// If we got removed after processing a control message we will receive a "removed" output
    /// signal.
    pub fn receive(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        message: &ORD::Message,
        rng: &Rng,
    ) -> GroupResult<Option<GroupOutput<ID, OP, DGM, ORD>>, ID, OP, PKI, DGM, KMG, ORD> {
        // Remember current members state so we can calculate a diff after processing these
        // messages.
        let members_pre = Self::members(&y)?;

        let message_type = message.content();
        let is_established = y.ratchet.is_some();
        let mut is_create_or_welcome = false;

        // Accept "create" control messages if we haven't established our state yet and if we are
        // part of the initial members set.
        if let ForwardSecureMessageContent::Control(ControlMessage::Create {
            ref initial_members,
        }) = message_type
        {
            if is_established {
                return Err(GroupError::GroupAlreadyEstablished);
            }

            if initial_members.contains(&y.my_id) {
                is_create_or_welcome = true;
            }
        }

        // Accept "add" control messages if we are being added by it.
        if let ForwardSecureMessageContent::Control(ControlMessage::Add { added }) = message_type {
            if !is_established && added == y.my_id {
                is_create_or_welcome = true;
            }
        }

        let y_orderer_i = ORD::queue(y.orderer, message).map_err(GroupError::Orderer)?;
        y.orderer = y_orderer_i;

        if !is_established && !is_create_or_welcome {
            // We're receiving control- and application messages for this group but we haven't
            // joined yet. We keep these messages for later. We don't know yet when we will join
            // the group and which of these messages we can process afterwards.
            return Ok((y, None));
        }

        if !is_established && is_create_or_welcome {
            // We've received a "create" or "add" (welcome) message for us and can join the group
            // now.
            let y_orderer_i = ORD::set_welcome(y.orderer, message).map_err(GroupError::Orderer)?;
            y.orderer = y_orderer_i;

            // Remember welcome message for later.
            y.welcome = Some(message.clone());

            // Always process welcome message first before anything else.
            let (y_i, result) = Self::process_ready(y, message, rng)?;

            let members_post = Self::members(&y_i)?;

            return Ok((
                y_i,
                result.map(|output| GroupOutput::new(vec![output], members_pre, members_post)),
            ));
        }

        let mut events = Vec::new();
        let mut y_loop = y;

        // Check if there's any correctly ordered messages ready to-be processed.
        loop {
            let (y_orderer_next, result) =
                ORD::next_ready_message(y_loop.orderer).map_err(GroupError::Orderer)?;
            y_loop.orderer = y_orderer_next;

            let Some(message) = result else {
                // Orderer is done yielding "ready" messages, stop here and try again later when we
                // receive new messages.
                break;
            };

            if let Some(welcome) = &y_loop.welcome {
                // Skip processing welcome again, we've already done that after receiving it.
                if welcome.id() == message.id() {
                    continue;
                }
            }

            let (y_next, result) = Self::process_ready(y_loop, &message, rng)?;
            y_loop = y_next;
            if let Some(message) = result {
                events.push(message);
            }
        }

        let members_post = Self::members(&y_loop)?;

        Ok((
            y_loop,
            Some(GroupOutput::new(events, members_pre, members_post)),
        ))
    }

    /// Encrypts application message towards the current group.
    ///
    /// The returned message can then be broadcast to all members in the group. The underlying
    /// protocol makes sure that all members will be able to decrypt this message.
    pub fn send(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        plaintext: &[u8],
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        let Some(y_ratchet) = y.ratchet else {
            return Err(GroupError::GroupNotYetEstablished);
        };

        // Encrypt application message.
        let (y_ratchet_i, generation, ciphertext) = Self::encrypt(y_ratchet, plaintext)?;
        y.ratchet = Some(y_ratchet_i);

        // Determine parameters for to-be-published application message.
        let (y_orderer_i, message) =
            ORD::next_application_message(y.orderer, generation, ciphertext)
                .map_err(GroupError::Orderer)?;
        y.orderer = y_orderer_i;

        Ok((y, message))
    }

    /// Returns a list of all current members in this group from our perspective.
    pub fn members(
        y: &GroupState<ID, OP, PKI, DGM, KMG, ORD>,
    ) -> Result<HashSet<ID>, GroupError<ID, OP, PKI, DGM, KMG, ORD>> {
        let members = Dcgka::member_view(&y.dcgka, &y.my_id)?;
        Ok(members)
    }

    /// Processes our own locally created control messages.
    fn process_local(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        output: OperationOutput<ID, OP, DGM>,
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        // Determine parameters for to-be-published control message.
        let (y_orderer_i, message) =
            ORD::next_control_message(y.orderer, &output.control_message, &output.direct_messages)
                .map_err(GroupError::Orderer)?;
        y.orderer = y_orderer_i;

        // Process control message locally to update our state.
        let (y_dcgka_i, process) = Dcgka::process_local(y.dcgka, message.id(), output, rng)?;
        y.dcgka = y_dcgka_i;

        // Update own encryption ratchet for sending application messages.
        y.ratchet = Some(RatchetSecret::init(
            process
                .me_update_secret
                .expect("local operation always yields an update secret for us")
                .into(),
        ));

        Ok((y, message))
    }

    /// Processes remote messages which have been marked as "ready" by the orderer.
    fn process_ready(
        y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        message: &ORD::Message,
        rng: &Rng,
    ) -> GroupResult<Option<GroupEvent<ID, OP, DGM, ORD>>, ID, OP, PKI, DGM, KMG, ORD> {
        match message.content() {
            ForwardSecureMessageContent::Control(control_message) => {
                let direct_message = message
                    .direct_messages()
                    .into_iter()
                    .find(|dm| dm.recipient == y.my_id);

                let (y_i, output) = Self::process_remote(
                    y,
                    message.id(),
                    message.sender(),
                    control_message,
                    direct_message,
                    rng,
                )?;

                // Check if processing this message removed us from the group.
                let is_removed = !Self::members(&y_i)?.contains(&y_i.my_id);
                if is_removed {
                    Ok((y_i, Some(GroupEvent::RemovedOurselves)))
                } else {
                    Ok((y_i, output.map(|msg| GroupEvent::Control(msg))))
                }
            }
            ForwardSecureMessageContent::Application {
                ciphertext,
                generation,
            } => {
                let (y_i, plaintext) = Self::decrypt(y, message.sender(), ciphertext, generation)?;
                Ok((
                    y_i,
                    Some(GroupEvent::Application {
                        plaintext,
                        message_id: message.id(),
                    }),
                ))
            }
        }
    }

    /// Internal method to process remote control message.
    fn process_remote(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        seq: OP,
        sender: ID,
        control_message: ControlMessage<ID, OP>,
        direct_message: Option<DirectMessage<ID, OP, DGM>>,
        rng: &Rng,
    ) -> GroupResult<Option<ORD::Message>, ID, OP, PKI, DGM, KMG, ORD> {
        let (y_dcgka_i, output) = Dcgka::process_remote(
            y.dcgka,
            ProcessInput {
                seq,
                sender,
                control_message,
                direct_message,
            },
            rng,
        )?;
        y.dcgka = y_dcgka_i;

        // Update own encryption ratchet for sending application messages.
        if let Some(me_update_secret) = output.me_update_secret {
            y.ratchet = Some(RatchetSecret::init(me_update_secret.into()));
        }

        // Update decryption ratchet for receiving application messages from this sender.
        if let Some(sender_update_secret) = output.sender_update_secret {
            y.decryption_ratchet
                .insert(sender, DecryptionRatchet::init(sender_update_secret.into()));
        }

        if let Some(output_control_message) = output.control_message {
            // Determine parameters for to-be-published control message.
            let (y_orderer_i, output_message) = ORD::next_control_message(
                y.orderer,
                &output_control_message,
                &output.direct_messages,
            )
            .map_err(GroupError::Orderer)?;
            y.orderer = y_orderer_i;
            Ok((y, Some(output_message)))
        } else {
            Ok((y, None))
        }
    }

    /// Encrypt message by using our ratchet.
    fn encrypt(
        y_ratchet: RatchetSecretState,
        plaintext: &[u8],
    ) -> Result<(RatchetSecretState, Generation, Vec<u8>), GroupError<ID, OP, PKI, DGM, KMG, ORD>>
    {
        // Derive key material to encrypt message from our ratchet.
        let (y_ratchet_i, generation, key_material) =
            RatchetSecret::ratchet_forward(y_ratchet).map_err(GroupError::EncryptionRatchet)?;

        // Encrypt message.
        let ciphertext = encrypt_message(plaintext, key_material)?;

        Ok((y_ratchet_i, generation, ciphertext))
    }

    /// Decrypt message by using the sender's ratchet.
    fn decrypt(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        sender: ID,
        ciphertext: Vec<u8>,
        generation: Generation,
    ) -> GroupResult<Vec<u8>, ID, OP, PKI, DGM, KMG, ORD> {
        let Some(y_decryption_ratchet) = y.decryption_ratchet.remove(&sender) else {
            return Err(GroupError::DecryptionRachetUnavailable(sender, generation));
        };

        // Try to derive required key material from ratchet.
        let (y_decryption_ratchet_i, key_material) = DecryptionRatchet::secret_for_decryption(
            y_decryption_ratchet,
            generation,
            y.config.maximum_forward_distance,
            y.config.out_of_order_tolerance,
        )
        .map_err(GroupError::DecryptionRatchet)?;
        y.decryption_ratchet.insert(sender, y_decryption_ratchet_i);

        // Attempt to decrypt message.
        let plaintext = decrypt_message(&ciphertext, key_material)?;

        Ok((y, plaintext))
    }
}

pub type GroupResult<T, ID, OP, PKI, DGM, KMG, ORD> =
    Result<(GroupState<ID, OP, PKI, DGM, KMG, ORD>, T), GroupError<ID, OP, PKI, DGM, KMG, ORD>>;

/// Result from processing a remote message or calling a local group operation.
#[derive(Clone, Default)]
pub struct GroupOutput<ID, OP, DGM, ORD>
where
    ID: Clone + Eq + PartialEq + StdHash,
    DGM: AckedGroupMembership<ID, OP>,
    ORD: ForwardSecureOrdering<ID, OP, DGM>,
{
    pub events: Vec<GroupEvent<ID, OP, DGM, ORD>>,
    pub removed_members: HashSet<ID>,
    pub added_members: HashSet<ID>,
}

impl<ID, OP, DGM, ORD> GroupOutput<ID, OP, DGM, ORD>
where
    ID: Clone + Eq + PartialEq + StdHash,
    DGM: AckedGroupMembership<ID, OP>,
    ORD: ForwardSecureOrdering<ID, OP, DGM>,
{
    pub(crate) fn new(
        events: Vec<GroupEvent<ID, OP, DGM, ORD>>,
        pre_members: HashSet<ID>,
        post_members: HashSet<ID>,
    ) -> Self {
        let added_members = post_members.difference(&pre_members).cloned().collect();
        let removed_members = pre_members.difference(&post_members).cloned().collect();

        GroupOutput {
            events,
            removed_members,
            added_members,
        }
    }
}

/// Dispatched event after processing a group message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupEvent<ID, OP, DGM, ORD>
where
    DGM: AckedGroupMembership<ID, OP>,
    ORD: ForwardSecureOrdering<ID, OP, DGM>,
{
    /// Control message for group encryption which should be broadcast to all members of the group.
    Control(ORD::Message),

    /// Decrypted payload of message.
    Application { plaintext: Vec<u8>, message_id: OP },

    /// Signal that we've been removed from the group.
    RemovedOurselves,
}

/// Configuration for the message ratchets used in this group.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupConfig {
    /// This parameter defines how many incoming messages can be skipped. This is useful if the
    /// application drops messages.
    pub maximum_forward_distance: u32,

    /// This parameter defines a window for which decryption secrets are kept. This is useful in
    /// case the ratchet cannot guarantee that all application messages have total order within an
    /// epoch. Use this carefully, since keeping decryption secrets affects forward secrecy within
    /// an epoch.
    pub out_of_order_tolerance: u32,
}

impl Default for GroupConfig {
    fn default() -> Self {
        Self {
            maximum_forward_distance: 1000,
            out_of_order_tolerance: 100,
        }
    }
}

#[derive(Debug, Error)]
pub enum GroupError<ID, OP, PKI, DGM, KMG, ORD>
where
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
    DGM: AckedGroupMembership<ID, OP>,
    KMG: PreKeyManager,
    ORD: ForwardSecureOrdering<ID, OP, DGM>,
{
    #[error(transparent)]
    Dcgka(#[from] DcgkaError<ID, OP, PKI, DGM, KMG>),

    #[error(transparent)]
    Orderer(ORD::Error),

    #[error(transparent)]
    EncryptionRatchet(RatchetError),

    #[error(transparent)]
    DecryptionRatchet(RatchetError),

    #[error(transparent)]
    Aead(#[from] AeadError),

    #[error("creating or joining a group is not possible, state is already established")]
    GroupAlreadyEstablished,

    #[error("state is not ready yet, group needs to be created or joined first")]
    GroupNotYetEstablished,

    #[error("can not add ourselves to the group")]
    NotAddOurselves,

    #[error("received \"create\" or \"add\" message addressing us but no direct message attached")]
    DirectMessageMissing,

    #[error("decryption ratchet not established yet to process the message from {0} @{1}")]
    DecryptionRachetUnavailable(ID, Generation),

    #[error("to-be-added member {0} is already part of the group")]
    AddedExistsAlready(ID),

    #[error("to-be-removed member {0} is not part of the group")]
    InexistentRemovedMember(ID),
}

#[cfg(test)]
mod tests {
    use crate::crypto::Rng;
    use crate::message_scheme::test_utils::network::Network;

    #[test]
    fn simple_group() {
        let alice = 0;
        let bob = 1;

        let mut network = Network::new([alice, bob], Rng::from_seed([1; 32]));

        // Alice creates a group with Bob.
        network.create(alice, vec![bob]);

        // Everyone processes each other's messages.
        let results = network.process();
        assert!(
            results.is_empty(),
            "no decrypted application messages expected"
        );

        // Alice and Bob share the same members view.
        for member in [alice, bob] {
            assert_eq!(network.members(&member), vec![alice, bob]);
        }

        // Alice sends a message to the group and Bob can decrypt it.
        network.send(alice, b"Hello everyone!");
        assert_eq!(
            network.process(),
            vec![(alice, bob, b"Hello everyone!".to_vec())],
        );
    }

    #[test]
    fn welcome() {
        let alice = 0;
        let bob = 1;
        let charlie = 2;

        let mut network = Network::new([alice, bob, charlie], Rng::from_seed([1; 32]));

        // Alice creates a group with Bob.
        network.create(alice, vec![bob]);
        network.process();

        // Bob updates the group.
        network.update(bob);
        network.process();

        // Bob sends a message to the group and Alice can decrypt it.
        network.send(bob, b"Huhu");
        assert_eq!(network.process(), vec![(bob, alice, b"Huhu".to_vec())],);

        // Bob adds Charlie. Charlie will process their "welcome" message now to join.
        network.add(bob, charlie);
        network.process();

        // Alice sends a message to the group and Bob and Charlie can decrypt it.
        network.send(alice, b"Hello everyone!");
        assert_eq!(
            network.process(),
            vec![
                (alice, bob, b"Hello everyone!".to_vec()),
                (alice, charlie, b"Hello everyone!".to_vec()),
            ],
        );
    }

    #[test]
    fn concurrency() {
        let alice = 0;
        let bob = 1;
        let charlie = 2;

        let mut network = Network::new([alice, bob, charlie], Rng::from_seed([1; 32]));

        // Alice creates a group with Bob.
        network.create(alice, vec![bob]);
        network.process();

        // Bob updates the group and concurrently Alice adds Charlie.
        network.update(bob);
        network.add(alice, charlie);
        network.process();

        // Bob sends a message to the group and Alice and Charlie can decrypt it.
        network.send(bob, b"Hello everyone!");
        assert_eq!(
            network.process(),
            vec![
                (bob, alice, b"Hello everyone!".to_vec()),
                (bob, charlie, b"Hello everyone!".to_vec()),
            ],
        );
    }

    #[test]
    fn removal() {
        let alice = 0;
        let bob = 1;
        let charlie = 2;

        let mut network = Network::new([alice, bob, charlie], Rng::from_seed([1; 32]));

        network.create(alice, vec![alice, bob, charlie]);
        network.process();

        // Alice removes Bob.
        network.remove(alice, bob);
        network.process();

        // Charlie removes themselves.
        network.remove(charlie, charlie);
        network.process();
    }
}
