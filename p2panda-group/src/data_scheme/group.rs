// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(clippy::type_complexity)]
use std::collections::{HashSet, VecDeque};
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::xchacha20::{XAeadError, XAeadNonce};
use crate::crypto::{Rng, RngError};
use crate::data_scheme::data::{decrypt_data, encrypt_data};
use crate::data_scheme::dcgka::{
    ControlMessage, Dcgka, DcgkaError, DcgkaState, DirectMessage, GroupSecretOutput,
    OperationOutput, ProcessInput,
};
use crate::data_scheme::group_secret::{
    GroupSecret, GroupSecretError, GroupSecretId, SecretBundle, SecretBundleState,
};
use crate::key_bundle::LongTermKeyBundle;
use crate::traits::{
    DataMessageType, EncryptedDataMessage, GroupMembership, IdentityHandle, IdentityManager,
    IdentityRegistry, OperationId, Ordering, PreKeyManager, PreKeyRegistry,
};

pub struct DataGroup<ID, OP, PKI, DGM, KMG, ORD> {
    _marker: PhantomData<(ID, OP, PKI, DGM, KMG, ORD)>,
}

/// Group state for "data encryption" scheme. Serializable for persistence.
#[derive(Debug, Serialize, Deserialize)]
pub struct GroupState<ID, OP, PKI, DGM, KMG, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, LongTermKeyBundle>,
    DGM: GroupMembership<ID, OP>,
    KMG: IdentityManager<KMG::State> + PreKeyManager,
    ORD: Ordering<ID, OP, DGM>,
{
    pub(crate) my_id: ID,
    pub(crate) dcgka: DcgkaState<ID, OP, PKI, DGM, KMG>,
    pub(crate) orderer: ORD::State,
    pub(crate) secrets: SecretBundleState,
    pub(crate) is_welcomed: bool,
}

impl<ID, OP, PKI, DGM, KMG, ORD> DataGroup<ID, OP, PKI, DGM, KMG, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, LongTermKeyBundle>,
    DGM: GroupMembership<ID, OP>,
    KMG: IdentityManager<KMG::State> + PreKeyManager,
    ORD: Ordering<ID, OP, DGM>,
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
    ) -> GroupState<ID, OP, PKI, DGM, KMG, ORD> {
        GroupState {
            my_id,
            dcgka: Dcgka::init(my_id, my_keys, pki, dgm),
            orderer,
            secrets: SecretBundle::init(),
            is_welcomed: false,
        }
    }

    /// Creates new group with initial set of members.
    pub fn create(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        initial_members: Vec<ID>,
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        if y.is_welcomed {
            return Err(GroupError::GroupAlreadyEstablished);
        }

        // Create new group with initial members.
        let (y_dcgka_i, pre) = Dcgka::create(y.dcgka, initial_members, rng)?;
        y.dcgka = y_dcgka_i;

        let (mut y_i, message) = Self::process_local(y, pre)?;

        // Set our own "create" as the "welcome" message.
        let y_orderer_i = ORD::set_welcome(y_i.orderer, &message).map_err(GroupError::Orderer)?;
        y_i.orderer = y_orderer_i;
        y_i.is_welcomed = true;

        Ok((y_i, message))
    }

    /// Adds new member to group.
    pub fn add(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        added: ID,
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        if !y.is_welcomed {
            return Err(GroupError::GroupNotYetEstablished);
        }

        if y.my_id == added {
            return Err(GroupError::NotAddOurselves);
        }

        // Add a new member to the group.
        let (y_dcgka_i, pre) = Dcgka::add(y.dcgka, added, &y.secrets, rng)?;
        y.dcgka = y_dcgka_i;

        Self::process_local(y, pre)
    }

    /// Removes member from group. It is possible to remove ourselves.
    pub fn remove(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        removed: ID,
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        if !y.is_welcomed {
            return Err(GroupError::GroupNotYetEstablished);
        }

        // Remove a member from the group.
        let (y_dcgka_i, pre) = Dcgka::remove(y.dcgka, removed, rng)?;
        y.dcgka = y_dcgka_i;

        Self::process_local(y, pre)
    }

    /// Updates group secret and provides all members with fresh entropy.
    pub fn update(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        if !y.is_welcomed {
            return Err(GroupError::GroupNotYetEstablished);
        }

        // Update the group by generating a new seed.
        let (y_dcgka_i, pre) = Dcgka::update(y.dcgka, rng)?;
        y.dcgka = y_dcgka_i;

        Self::process_local(y, pre)
    }

    /// Handler for incoming, remote messages.
    ///
    /// This yields a list of "outputs" which can be either control messages which need to be
    /// broadcast to all members in the group or decrypted application payloads.
    ///
    /// If we got removed after processing a control message we will receive an "removed" output
    /// signal.
    pub fn receive(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        message: &ORD::Message,
    ) -> GroupResult<Vec<ReceiveOutput<ID, OP, DGM, ORD>>, ID, OP, PKI, DGM, KMG, ORD> {
        let message_type = message.message_type();
        let mut is_create_or_welcome = false;

        // Accept "create" control messages if we haven't established our state yet and if we are
        // part of the initial members set.
        if let DataMessageType::Control(ControlMessage::Create {
            ref initial_members,
        }) = message_type
        {
            if y.is_welcomed {
                return Err(GroupError::GroupAlreadyEstablished);
            }

            if initial_members.contains(&y.my_id) {
                is_create_or_welcome = true;
            }
        }

        // Accept "add" control messages if we are being added by it.
        if let DataMessageType::Control(ControlMessage::Add { added }) = message_type {
            if !y.is_welcomed && added == y.my_id {
                is_create_or_welcome = true;
            }
        }

        let y_orderer_i = ORD::queue(y.orderer, message).map_err(GroupError::Orderer)?;
        y.orderer = y_orderer_i;

        if !y.is_welcomed && !is_create_or_welcome {
            // We're receiving control- and application messages for this group but we haven't
            // joined yet. We keep these messages for later. We don't know yet when we will join
            // the group and which of these messages we can process afterwards.
            return Ok((y, vec![]));
        }

        if !y.is_welcomed && is_create_or_welcome {
            // We've received a "create" or "add" (welcome) message for us and can join the group
            // now.
            let y_orderer_i = ORD::set_welcome(y.orderer, message).map_err(GroupError::Orderer)?;
            y.orderer = y_orderer_i;
        }

        let mut results = Vec::new();
        let mut y_loop = y;

        let mut control_messages = VecDeque::new();
        let mut application_messages = VecDeque::new();

        // Check if there's any correctly ordered messages ready to-be processed.
        loop {
            let (y_orderer_next, result) =
                ORD::next_ready_message(y_loop.orderer).map_err(GroupError::Orderer)?;
            y_loop.orderer = y_orderer_next;

            let Some(message) = result else {
                break;
            };

            match message.message_type() {
                DataMessageType::Control(_) => {
                    control_messages.push_back(message);
                }
                DataMessageType::Application { .. } => {
                    application_messages.push_back(message);
                }
            }
        }

        // Process all control messages first.
        while let Some(message) = control_messages.pop_front() {
            let (y_next, result) = Self::process_ready(y_loop, &message)?;
            y_loop = y_next;
            if let Some(message) = result {
                results.push(message);
            }
        }

        // .. then process all application messages.
        while let Some(message) = application_messages.pop_front() {
            let (y_next, result) = Self::process_ready(y_loop, &message)?;
            y_loop = y_next;
            if let Some(message) = result {
                results.push(message);
            }
        }

        Ok((y_loop, results))
    }

    /// Encrypts application payload towards the current group.
    ///
    /// The returned message can then be broadcast to all members in the group. The underlying
    /// protocol makes sure that all members will be able to decrypt this message.
    pub fn send(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        plaintext: &[u8],
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        if !y.is_welcomed {
            return Err(GroupError::GroupNotYetEstablished);
        }

        let Some(group_secret) = y.secrets.latest() else {
            return Err(GroupError::NoGroupSecretAvailable);
        };

        // Encrypt application data.
        let secret_id = group_secret.id();
        let (nonce, ciphertext) = Self::encrypt(group_secret, plaintext, rng)?;

        // Determine parameters for to-be-published application message.
        let (y_orderer_i, message) =
            ORD::next_application_message(y.orderer, secret_id, nonce, ciphertext)
                .map_err(GroupError::Orderer)?;
        y.orderer = y_orderer_i;

        Ok((y, message))
    }

    /// Returns a list of all current members in this group from our perspective.
    pub fn members(
        y: &GroupState<ID, OP, PKI, DGM, KMG, ORD>,
    ) -> Result<HashSet<ID>, GroupError<ID, OP, PKI, DGM, KMG, ORD>> {
        let members = Dcgka::members(&y.dcgka)?;
        Ok(members)
    }

    /// Processes our own locally created control messages.
    fn process_local(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        output: OperationOutput<ID, OP, DGM>,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        // Determine parameters for to-be-published control message.
        let (y_orderer_i, message) =
            ORD::next_control_message(y.orderer, &output.control_message, &output.direct_messages)
                .map_err(GroupError::Orderer)?;
        y.orderer = y_orderer_i;

        // Process control message locally to update our state.
        let (y_dcgka_i, _) = Dcgka::process(
            y.dcgka,
            ProcessInput {
                seq: message.id(),
                sender: message.sender(),
                control_message: output.control_message,
                direct_message: None,
            },
        )?;
        y.dcgka = y_dcgka_i;

        // Add new generated secret to bundle.
        y.secrets = SecretBundle::insert(
            y.secrets,
            output
                .group_secret
                .expect("local operations always yield a group secret"),
        );

        Ok((y, message))
    }

    /// Processes remote messages which have been marked as "ready" by the orderer.
    fn process_ready(
        y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        message: &ORD::Message,
    ) -> GroupResult<Option<ReceiveOutput<ID, OP, DGM, ORD>>, ID, OP, PKI, DGM, KMG, ORD> {
        match message.message_type() {
            DataMessageType::Control(control_message) => {
                let direct_message = message
                    .direct_messages()
                    .into_iter()
                    .find(|dm| dm.recipient == y.my_id);

                let (mut y_i, output) = Self::process_remote(
                    y,
                    message.id(),
                    message.sender(),
                    control_message,
                    direct_message,
                )?;

                // Check if processing this message added us to the group.
                let we_are_members = Self::members(&y_i)?.contains(&y_i.my_id);
                if !y_i.is_welcomed && we_are_members {
                    y_i.is_welcomed = true;
                }

                // Check if processing this message removed us from the group.
                let is_removed = y_i.is_welcomed && !we_are_members;
                if is_removed {
                    Ok((y_i, Some(ReceiveOutput::Removed)))
                } else {
                    Ok((y_i, output.map(|msg| ReceiveOutput::Control(msg))))
                }
            }
            DataMessageType::Application {
                group_secret_id,
                ciphertext,
                nonce,
            } => {
                let (y_i, plaintext) = Self::decrypt(y, nonce, group_secret_id, ciphertext)?;
                Ok((y_i, Some(ReceiveOutput::Application { plaintext })))
            }
        }
    }

    /// Internal method to process remote control message.
    fn process_remote(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        seq: OP,
        sender: ID,
        control_message: ControlMessage<ID>,
        direct_message: Option<DirectMessage<ID, OP, DGM>>,
    ) -> GroupResult<Option<ORD::Message>, ID, OP, PKI, DGM, KMG, ORD> {
        let (y_dcgka_i, output) = Dcgka::process(
            y.dcgka,
            ProcessInput {
                seq,
                sender,
                control_message,
                direct_message,
            },
        )?;
        y.dcgka = y_dcgka_i;

        // Add newly learned group secrets to our bundle.
        y.secrets = match output.group_secret {
            GroupSecretOutput::Secret(group_secret) => {
                SecretBundle::insert(y.secrets, group_secret)
            }
            GroupSecretOutput::Bundle(secret_bundle_state) => {
                SecretBundle::extend(y.secrets, secret_bundle_state)
            }
            GroupSecretOutput::None => y.secrets,
        };

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

    /// Encrypt message by using the latest known group secret and a random nonce.
    fn encrypt(
        group_secret: &GroupSecret,
        plaintext: &[u8],
        rng: &Rng,
    ) -> Result<(XAeadNonce, Vec<u8>), GroupError<ID, OP, PKI, DGM, KMG, ORD>> {
        let nonce: XAeadNonce = rng.random_array()?;
        let ciphertext = encrypt_data(plaintext, group_secret, nonce)?;
        Ok((nonce, ciphertext))
    }

    /// Decrypt message by using a group secret.
    fn decrypt(
        y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        nonce: XAeadNonce,
        group_secret_id: GroupSecretId,
        ciphertext: Vec<u8>,
    ) -> GroupResult<Vec<u8>, ID, OP, PKI, DGM, KMG, ORD> {
        let Some(group_secret) = y.secrets.get(&group_secret_id) else {
            return Err(GroupError::UnknownGroupSecret(hex::encode(group_secret_id)));
        };
        let plaintext = decrypt_data(&ciphertext, group_secret, nonce)?;
        Ok((y, plaintext))
    }
}

pub type GroupResult<T, ID, OP, PKI, DGM, KMG, ORD> =
    Result<(GroupState<ID, OP, PKI, DGM, KMG, ORD>, T), GroupError<ID, OP, PKI, DGM, KMG, ORD>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceiveOutput<ID, OP, DGM, ORD>
where
    DGM: GroupMembership<ID, OP>,
    ORD: Ordering<ID, OP, DGM>,
{
    /// Control message for group encryption which should be broadcast to all members of the group.
    Control(ORD::Message),

    /// Decrypted payload of message.
    Application { plaintext: Vec<u8> },

    /// Signal that we've been removed from the group.
    Removed,
}

#[derive(Debug, Error)]
pub enum GroupError<ID, OP, PKI, DGM, KMG, ORD>
where
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, LongTermKeyBundle>,
    DGM: GroupMembership<ID, OP>,
    KMG: PreKeyManager,
    ORD: Ordering<ID, OP, DGM>,
{
    #[error(transparent)]
    Rng(#[from] RngError),

    #[error(transparent)]
    Dcgka(#[from] DcgkaError<ID, OP, PKI, DGM, KMG>),

    #[error(transparent)]
    Orderer(ORD::Error),

    #[error(transparent)]
    XAead(#[from] XAeadError),

    #[error(transparent)]
    GroupSecret(#[from] GroupSecretError),

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

    #[error("we do not have created or learned about any group secrets yet")]
    NoGroupSecretAvailable,

    #[error("tried to decrypt message with an unknown group secret: {0}")]
    UnknownGroupSecret(String),
}
