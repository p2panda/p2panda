// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::marker::PhantomData;

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
    AckedGroupMembership, ForwardSecureOrdering, IdentityHandle, IdentityManager, IdentityRegistry,
    MessageInfo, MessageType, OperationId, PreKeyManager, PreKeyRegistry,
};

pub struct MessageGroup<ID, OP, PKI, DGM, KMG, ORD> {
    _marker: PhantomData<(ID, OP, PKI, DGM, KMG, ORD)>,
}

pub struct GroupState<ID, OP, PKI, DGM, KMG, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
    DGM: AckedGroupMembership<ID, OP>,
    KMG: IdentityManager<KMG::State> + PreKeyManager,
    ORD: ForwardSecureOrdering<ID, OP, DGM>,
{
    my_id: ID,
    dcgka: DcgkaState<ID, OP, PKI, DGM, KMG>,
    orderer: ORD::State,
    ratchet: Option<RatchetSecretState>,
    decryption_ratchet: HashMap<ID, DecryptionRatchetState>,
    config: GroupConfig,
}

impl<ID, OP, PKI, DGM, KMG, ORD> MessageGroup<ID, OP, PKI, DGM, KMG, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
    DGM: AckedGroupMembership<ID, OP>,
    KMG: IdentityManager<KMG::State> + PreKeyManager,
    ORD: ForwardSecureOrdering<ID, OP, DGM>,
{
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
            ratchet: None,
            decryption_ratchet: HashMap::new(),
            config,
        }
    }

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

        Ok(Self::process_local(y, pre, rng)?)
    }

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

        // Add a new member to the group.
        let (y_dcgka_i, pre) = Dcgka::add(y.dcgka, added, rng)?;
        y.dcgka = y_dcgka_i;

        Ok(Self::process_local(y, pre, rng)?)
    }

    pub fn remove(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        removed: ID,
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        if y.ratchet.is_none() {
            return Err(GroupError::GroupNotYetEstablished);
        }

        // Remove a member from the group.
        let (y_dcgka_i, pre) = Dcgka::remove(y.dcgka, removed, rng)?;
        y.dcgka = y_dcgka_i;

        // TODO: Handle removing ourselves.

        Ok(Self::process_local(y, pre, rng)?)
    }

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

        Ok(Self::process_local(y, pre, rng)?)
    }

    pub fn receive(
        y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        message: &ORD::Message,
        rng: &Rng,
    ) -> GroupResult<Vec<ReceiveOutput<ID, OP, DGM, ORD>>, ID, OP, PKI, DGM, KMG, ORD> {
        let message_type = message.message_type();
        let is_established = y.ratchet.is_some();
        let mut is_create_or_welcome = false;

        // Accept "create" control messages if we haven't established our state yet and if we are
        // part of the initial members set.
        if let MessageType::Control(ControlMessage::Create {
            ref initial_members,
        }) = message_type
        {
            if is_established {
                return Err(GroupError::GroupAlreadyEstablished);
            }

            if !initial_members.contains(&y.my_id) {
                return Err(GroupError::CreateNotForUs);
            }

            is_create_or_welcome = true;
        }

        // Accept "add" control messages if we either are being added by it or if we already have a
        // group state established (and someone else is being added).
        if let MessageType::Control(ControlMessage::Add { added }) = message_type {
            if !is_established && added != y.my_id {
                return Err(GroupError::WelcomeNotForUs);
            }

            if !is_established && added == y.my_id {
                is_create_or_welcome = true;
            }
        }

        // TODO: Message ordering.

        let (y_i, output) = match (is_established, is_create_or_welcome) {
            (false, false) => {
                // 1. We're receiving control- and application-messages for this group but we
                //    haven't joined yet. We keep these messages for later. We don't know yet when
                //    we will join the group and which of these messages we can process afterwards.
                // TODO
                (y, vec![])
            }
            (false, true) => {
                // 2. We've received a "create" or "add" (welcome) message for us and can join the
                //    group now.
                let direct_message = message
                    .direct_messages()
                    .into_iter()
                    .find(|dm| dm.recipient == y.my_id);

                if direct_message.is_none() {
                    return Err(GroupError::DirectMessageMissing);
                }

                let MessageType::Control(control_message) = message_type else {
                    unreachable!();
                };

                let (y_i, output) = Self::process_remote(
                    y,
                    message.id(),
                    message.sender(),
                    control_message,
                    direct_message,
                    rng,
                )?;

                // This "create" or "add" control message established the group state for us. We
                // can now process all messages we've kept around before.
                // TODO

                (
                    y_i,
                    output.map_or(vec![], |msg| vec![ReceiveOutput::Control(msg)]),
                )
            }
            (true, false) => match message_type {
                // 3. We've received an "update", "add", "remove", "ack" or "add_ack" control
                //    message and process it. This can potentially yield a new control message
                //    ("ack" or "add_ack") we need to publish.
                MessageType::Control(control_message) => {
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

                    // TODO: Detect if this message removed us.

                    (
                        y_i,
                        output.map_or(vec![], |msg| vec![ReceiveOutput::Control(msg)]),
                    )
                }
                // 4. We've received an application message from the group to decrypt.
                MessageType::Application {
                    ciphertext,
                    generation,
                } => {
                    let (y_i, plaintext) =
                        Self::decrypt(y, message.sender(), ciphertext, generation)?;
                    (y_i, vec![ReceiveOutput::Application { plaintext }])
                }
            },
            (true, true) => {
                unreachable!("we should have handled this case before");
            }
        };

        Ok((y_i, output))
    }

    pub fn send(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        plaintext: &[u8],
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        let Some(y_ratchet) = y.ratchet else {
            return Err(GroupError::GroupNotYetEstablished);
        };

        // Derive key material to encrypt message from our ratchet.
        let (y_ratchet_i, generation, key_material) =
            RatchetSecret::ratchet_forward(y_ratchet).map_err(GroupError::EncryptionRatchet)?;
        y.ratchet = Some(y_ratchet_i);

        // Encrypt message.
        let ciphertext = encrypt_message(plaintext, key_material)?;

        // Determine parameters for to-be-published application message.
        let (y_orderer_i, message) =
            ORD::next_application_message(y.orderer, generation, ciphertext)
                .map_err(GroupError::Orderer)?;
        y.orderer = y_orderer_i;

        Ok((y, message))
    }

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
            y.config.ooo_tolerance,
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

pub enum ReceiveOutput<ID, OP, DGM, ORD>
where
    DGM: AckedGroupMembership<ID, OP>,
    ORD: ForwardSecureOrdering<ID, OP, DGM>,
{
    Control(ORD::Message),
    Application { plaintext: Vec<u8> },
}

pub struct GroupConfig {
    pub maximum_forward_distance: u32,
    pub ooo_tolerance: u32,
}

impl Default for GroupConfig {
    fn default() -> Self {
        Self {
            maximum_forward_distance: 1000,
            ooo_tolerance: 100,
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

    #[error("received a \"create\" control message which is not for us")]
    CreateNotForUs,

    #[error("received an \"add\" control message (welcome) which is not for us")]
    WelcomeNotForUs,

    #[error("received \"create\" or \"add\" message addressing us but no direct message attached")]
    DirectMessageMissing,

    #[error(
        "we do not have a decryption ratchet established yet to process the message from {0} @{1}"
    )]
    DecryptionRachetUnavailable(ID, Generation),
}
