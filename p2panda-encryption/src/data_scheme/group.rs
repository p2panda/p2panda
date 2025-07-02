// SPDX-License-Identifier: MIT OR Apache-2.0

//! API to manage groups using the "Data Encryption" scheme and process remote control- and
//! application messages.
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
    GroupMembership, GroupMessage, GroupMessageContent, IdentityHandle, IdentityManager,
    IdentityRegistry, OperationId, Ordering, PreKeyManager, PreKeyRegistry,
};

/// API to manage groups using the "Data Encryption" scheme and process remote control messages.
pub struct EncryptionGroup<ID, OP, PKI, DGM, KMG, ORD> {
    _marker: PhantomData<(ID, OP, PKI, DGM, KMG, ORD)>,
}

/// Group state for "data encryption" scheme. Serializable for persistence.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct GroupState<ID, OP, PKI, DGM, KMG, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, LongTermKeyBundle>,
    PKI::State: Clone,
    DGM: GroupMembership<ID, OP>,
    KMG: IdentityManager<KMG::State> + PreKeyManager,
    KMG::State: Clone,
    ORD: Ordering<ID, OP, DGM>,
{
    pub(crate) my_id: ID,
    pub(crate) dcgka: DcgkaState<ID, OP, PKI, DGM, KMG>,
    pub(crate) orderer: ORD::State,
    pub(crate) secrets: SecretBundleState,
    pub(crate) is_welcomed: bool,
}

impl<ID, OP, PKI, DGM, KMG, ORD> EncryptionGroup<ID, OP, PKI, DGM, KMG, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, LongTermKeyBundle>,
    PKI::State: Clone,
    DGM: GroupMembership<ID, OP>,
    KMG: IdentityManager<KMG::State> + PreKeyManager,
    KMG::State: Clone,
    ORD: Ordering<ID, OP, DGM>,
{
    /// Returns initial state for group.
    ///
    /// This needs to be called before creating or being added to a group.
    #[allow(unused)]
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

        // Generate new group secret.
        let group_secret = SecretBundle::generate(&y.secrets, rng)?;

        // Create new group with initial members.
        let (y_dcgka_i, pre) = Dcgka::create(y.dcgka, initial_members, &group_secret, rng)?;
        y.dcgka = y_dcgka_i;

        let (mut y_i, message) = Self::process_local(y, pre, Some(group_secret))?;

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

        Self::process_local(y, pre, None)
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

        // Generate new group secret.
        let group_secret = SecretBundle::generate(&y.secrets, rng)?;

        // Remove a member from the group.
        let (y_dcgka_i, pre) = Dcgka::remove(y.dcgka, removed, &group_secret, rng)?;
        y.dcgka = y_dcgka_i;

        Self::process_local(y, pre, Some(group_secret))
    }

    /// Updates group by providing all current members with new group secret.
    pub fn update(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        if !y.is_welcomed {
            return Err(GroupError::GroupNotYetEstablished);
        }

        // Generate new group secret.
        let group_secret = SecretBundle::generate(&y.secrets, rng)?;

        // Update the group by generating a new seed.
        let (y_dcgka_i, pre) = Dcgka::update(y.dcgka, &group_secret, rng)?;
        y.dcgka = y_dcgka_i;

        Self::process_local(y, pre, Some(group_secret))
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
    ) -> GroupResult<Vec<GroupOutput<ID, OP, DGM, ORD>>, ID, OP, PKI, DGM, KMG, ORD> {
        let message_content = message.content();
        let mut is_create_or_welcome = false;

        // Accept "create" control messages if we haven't established our state yet and if we are
        // part of the initial members set.
        if let GroupMessageContent::Control(ControlMessage::Create {
            ref initial_members,
        }) = message_content
        {
            if y.is_welcomed {
                return Err(GroupError::GroupAlreadyEstablished);
            }

            if initial_members.contains(&y.my_id) {
                is_create_or_welcome = true;
            }
        }

        // Accept "add" control messages if we are being added by it.
        if let GroupMessageContent::Control(ControlMessage::Add { added }) = message_content {
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

            match message.content() {
                GroupMessageContent::Control(_) => {
                    control_messages.push_back(message);
                }
                GroupMessageContent::Application { .. } => {
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

    /// Applications can remove group secrets for forward secrecy based on their own logic.
    ///
    /// Make sure that the ordering implementation and higher-level application logic accounts for
    /// error cases where past secrets might not exist anymore.
    #[allow(unused)]
    pub fn update_secrets<F>(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        update_fn: F,
    ) -> GroupState<ID, OP, PKI, DGM, KMG, ORD>
    where
        F: FnOnce(SecretBundleState) -> SecretBundleState,
    {
        y.secrets = update_fn(y.secrets);
        y
    }

    /// Processes our own locally created control messages.
    fn process_local(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        output: OperationOutput<ID, OP, DGM>,
        group_secret: Option<GroupSecret>,
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

        // Add new generated secret to bundle when given.
        if let Some(group_secret) = group_secret {
            y.secrets = SecretBundle::insert(y.secrets, group_secret);
        }

        Ok((y, message))
    }

    /// Processes remote messages which have been marked as "ready" by the orderer.
    fn process_ready(
        y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        message: &ORD::Message,
    ) -> GroupResult<Option<GroupOutput<ID, OP, DGM, ORD>>, ID, OP, PKI, DGM, KMG, ORD> {
        match message.content() {
            GroupMessageContent::Control(control_message) => {
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

                // y.secrets = SecretBundle::insert(y.secrets, new_group_secret);

                // Check if processing this message added us to the group.
                let we_are_members = Self::members(&y_i)?.contains(&y_i.my_id);
                if !y_i.is_welcomed && we_are_members {
                    y_i.is_welcomed = true;
                }

                // Check if processing this message removed us from the group.
                let is_removed = y_i.is_welcomed && !we_are_members;
                if is_removed {
                    Ok((y_i, Some(GroupOutput::Removed)))
                } else {
                    Ok((y_i, output.map(|msg| GroupOutput::Control(msg))))
                }
            }
            GroupMessageContent::Application {
                group_secret_id,
                ciphertext,
                nonce,
            } => {
                let (y_i, plaintext) = Self::decrypt(y, nonce, group_secret_id, ciphertext)?;
                Ok((y_i, Some(GroupOutput::Application { plaintext })))
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
        y.secrets = match output {
            GroupSecretOutput::Secret(group_secret) => {
                SecretBundle::insert(y.secrets, group_secret)
            }
            GroupSecretOutput::Bundle(secret_bundle_state) => {
                SecretBundle::extend(y.secrets, secret_bundle_state)
            }
            GroupSecretOutput::None => y.secrets,
        };

        // Processing remote control messages never results in new messages.
        Ok((y, None))
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

/// Result from processing a remote message or calling a local group operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupOutput<ID, OP, DGM, ORD>
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

    #[error("we do not have created or learned about any group secrets yet")]
    NoGroupSecretAvailable,

    #[error("tried to decrypt message with an unknown group secret: {0}")]
    UnknownGroupSecret(String),
}

#[cfg(test)]
mod tests {
    use crate::crypto::Rng;
    use crate::data_scheme::group::GroupOutput;
    use crate::data_scheme::test_utils::network::init_group_state;
    use crate::traits::{GroupMembership, Ordering};

    use super::{EncryptionGroup, GroupError};

    pub fn assert_payload<ID, OP, DGM, ORD>(
        messages: &[GroupOutput<ID, OP, DGM, ORD>],
        expected_payload: &[u8],
    ) where
        DGM: GroupMembership<ID, OP>,
        ORD: Ordering<ID, OP, DGM>,
    {
        let message = messages.first().expect("expected at least one message");
        if let GroupOutput::Application { plaintext } = message {
            assert_eq!(
                plaintext, expected_payload,
                "expected payload does not match"
            );
        } else {
            panic!("expected application message");
        }
    }

    #[test]
    fn post_compromise_security() {
        let rng = Rng::from_seed([1; 32]);

        let alice = 0;
        let bob = 1;
        let charlie = 2;

        let [y_alice, y_bob, y_charlie] = init_group_state([alice, bob, charlie], &rng);

        // Alice creates a group with Bob and Charlie.
        let (y_alice, alice_message_0) =
            EncryptionGroup::create(y_alice, vec![alice, bob, charlie], &rng).unwrap();
        let (y_bob, _) = EncryptionGroup::receive(y_bob, &alice_message_0).unwrap();
        let (y_charlie, _) = EncryptionGroup::receive(y_charlie, &alice_message_0).unwrap();

        // Alice encrypts data for Bob and Charlie.
        let (y_alice, alice_message_1) = EncryptionGroup::send(y_alice, b"Da Da Da", &rng).unwrap();

        // Both Bob and Charlie can decrypt the payload.
        let (y_bob, bob_output) = EncryptionGroup::receive(y_bob, &alice_message_1).unwrap();
        assert_payload(&bob_output, b"Da Da Da");
        let (y_charlie, charlie_output) =
            EncryptionGroup::receive(y_charlie, &alice_message_1).unwrap();
        assert_payload(&charlie_output, b"Da Da Da");

        // Bob removes Charlie.
        let (y_bob, bob_message_0) = EncryptionGroup::remove(y_bob, charlie, &rng).unwrap();
        let (y_alice, alice_output) = EncryptionGroup::receive(y_alice, &bob_message_0).unwrap();
        assert!(alice_output.is_empty());

        // Alice and Bob should have both the same "latest" secret.
        assert_eq!(
            y_alice.secrets.latest().unwrap().id(),
            y_bob.secrets.latest().unwrap().id()
        );

        // Charlie receives the signal that they got removed by this message.
        let (y_charlie, charlie_output) =
            EncryptionGroup::receive(y_charlie, &bob_message_0).unwrap();
        let GroupOutput::Removed = charlie_output.first().unwrap() else {
            panic!("expected removed output");
        };

        // Alice encrypts data for Bob.
        let (_y_alice, alice_message_2) =
            EncryptionGroup::send(y_alice, b"Ich lieb dich nicht / Du liebst mich nicht", &rng)
                .unwrap();

        // Bob can still decrypt this message from Alice.
        let (_y_bob, bob_output) = EncryptionGroup::receive(y_bob, &alice_message_2).unwrap();
        assert_payload(&bob_output, b"Ich lieb dich nicht / Du liebst mich nicht");

        // Charlie can not decrypt the latest message anymore.
        assert!(matches!(
            EncryptionGroup::receive(y_charlie, &alice_message_2),
            Err(GroupError::UnknownGroupSecret(_))
        ));
    }
}
