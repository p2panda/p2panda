// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
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
    AckedGroupMembership, ForwardSecureOrdering, IdentityHandle, IdentityManager, IdentityRegistry,
    MessageInfo, MessageType, OperationId, PreKeyManager, PreKeyRegistry,
};

pub struct MessageGroup<ID, OP, PKI, DGM, KMG, ORD> {
    _marker: PhantomData<(ID, OP, PKI, DGM, KMG, ORD)>,
}

#[derive(Debug, Serialize, Deserialize)]
// TODO: Derive Clone here for testing
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

        // TODO: Set welcome info in orderer.

        Self::process_local(y, pre, rng)
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

        Self::process_local(y, pre, rng)
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

        Self::process_local(y, pre, rng)
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

        Self::process_local(y, pre, rng)
    }

    pub fn receive(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
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

            if initial_members.contains(&y.my_id) {
                is_create_or_welcome = true;
            }
        }

        // Accept "add" control messages if we are being added by it.
        if let MessageType::Control(ControlMessage::Add { added }) = message_type {
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
            return Ok((y, vec![]));
        }

        if !is_established && is_create_or_welcome {
            // We've received a "create" or "add" (welcome) message for us and can join the group
            // now.
            let y_orderer_i = ORD::set_welcome(y.orderer, message).map_err(GroupError::Orderer)?;
            y.orderer = y_orderer_i;
        }

        // Check if there's any correctly ordered messages ready to-be processed.
        let mut results = Vec::new();
        let mut y_loop = y;
        loop {
            let (y_orderer_next, result) =
                ORD::next_ready_message(y_loop.orderer).map_err(GroupError::Orderer)?;
            y_loop.orderer = y_orderer_next;
            let Some(message) = result else {
                break;
            };

            let (y_next, result) = Self::process_ready(y_loop, &message, rng)?;
            y_loop = y_next;
            if let Some(message) = result {
                results.push(message);
            }
        }
        Ok((y_loop, results))
    }

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

    pub fn members(
        y: &GroupState<ID, OP, PKI, DGM, KMG, ORD>,
    ) -> Result<HashSet<ID>, GroupError<ID, OP, PKI, DGM, KMG, ORD>> {
        let members = Dcgka::member_view(&y.dcgka, &y.my_id)?;
        Ok(members)
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

        // TODO: Add message to orderer?;

        Ok((y, message))
    }

    fn process_ready(
        y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        message: &ORD::Message,
        rng: &Rng,
    ) -> GroupResult<Option<ReceiveOutput<ID, OP, DGM, ORD>>, ID, OP, PKI, DGM, KMG, ORD> {
        match message.message_type() {
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

                Ok((y_i, output.map(|msg| ReceiveOutput::Control(msg))))
            }
            MessageType::Application {
                ciphertext,
                generation,
            } => {
                let (y_i, plaintext) = Self::decrypt(y, message.sender(), ciphertext, generation)?;
                Ok((y_i, Some(ReceiveOutput::Application { plaintext })))
            }
        }
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceiveOutput<ID, OP, DGM, ORD>
where
    DGM: AckedGroupMembership<ID, OP>,
    ORD: ForwardSecureOrdering<ID, OP, DGM>,
{
    Control(ORD::Message),
    Application { plaintext: Vec<u8> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};

    use crate::message_scheme::acked_dgm::test_utils::AckedTestDGM;
    use crate::message_scheme::ordering::test_utils::{TestMessage, TestOrderer};
    use crate::message_scheme::test_utils::{MemberId, MessageId, init_dcgka_state};
    use crate::traits::MessageInfo;
    use crate::{KeyManager, KeyRegistry, Rng};

    use super::{GroupConfig, GroupState, MessageGroup, ReceiveOutput};

    type TestGroupState = GroupState<
        MemberId,
        MessageId,
        KeyRegistry<MemberId>,
        AckedTestDGM<MemberId, MessageId>,
        KeyManager,
        TestOrderer<AckedTestDGM<MemberId, MessageId>>,
    >;

    struct Network {
        rng: Rng,
        members: HashMap<MemberId, TestGroupState>,
        queue: VecDeque<TestMessage<AckedTestDGM<MemberId, MessageId>>>,
    }

    impl Network {
        pub fn new<const N: usize>(members: [MemberId; N], rng: Rng) -> Self {
            let members = init_dcgka_state(members, &rng);
            Self {
                members: HashMap::from_iter(members.into_iter().map(|dcgka| {
                    (dcgka.my_id, {
                        let orderer =
                            TestOrderer::<AckedTestDGM<MemberId, MessageId>>::init(dcgka.my_id);
                        TestGroupState {
                            my_id: dcgka.my_id,
                            dcgka,
                            orderer,
                            ratchet: None,
                            decryption_ratchet: HashMap::new(),
                            config: GroupConfig::default(),
                        }
                    })
                })),
                rng,
                queue: VecDeque::new(),
            }
        }

        pub fn create(&mut self, creator: MemberId, initial_members: Vec<MemberId>) {
            let y = self.get_y(&creator);
            let (y_i, message) = MessageGroup::create(y, initial_members, &self.rng).unwrap();
            self.queue.push_back(message);
            self.set_y(y_i);
        }

        pub fn add(&mut self, adder: MemberId, added: MemberId) {
            let y = self.get_y(&adder);
            let (y_i, message) = MessageGroup::add(y, added, &self.rng).unwrap();
            self.queue.push_back(message);
            self.set_y(y_i);
        }

        pub fn remove(&mut self, remover: MemberId, removed: MemberId) {
            let y = self.get_y(&remover);
            let (y_i, message) = MessageGroup::remove(y, removed, &self.rng).unwrap();
            self.queue.push_back(message);
            self.set_y(y_i);
        }

        pub fn update(&mut self, updater: MemberId) {
            let y = self.get_y(&updater);
            let (y_i, message) = MessageGroup::update(y, &self.rng).unwrap();
            self.queue.push_back(message);
            self.set_y(y_i);
        }

        pub fn send(&mut self, sender: MemberId, plaintext: &[u8]) {
            let y = self.get_y(&sender);
            let (y_i, message) = MessageGroup::send(y, plaintext).unwrap();
            self.queue.push_back(message);
            self.set_y(y_i);
        }

        pub fn process(&mut self) -> Vec<(MessageId, Vec<u8>)> {
            if self.queue.is_empty() {
                return Vec::new();
            }

            let mut decrypted_messages = Vec::new();
            let member_ids: Vec<MemberId> = self.members.keys().cloned().collect();

            while let Some(message) = self.queue.pop_front() {
                for id in &member_ids {
                    // Do not process our own messages.
                    if &message.sender() == id {
                        continue;
                    }

                    let y = self.get_y(id);
                    let (y_i, result) = MessageGroup::receive(y, &message, &self.rng).unwrap();
                    self.set_y(y_i);

                    for output in result {
                        match output {
                            ReceiveOutput::Control(control_message) => {
                                self.queue.push_back(control_message);
                            }
                            ReceiveOutput::Application { plaintext } => {
                                decrypted_messages.push((message.id(), plaintext))
                            }
                        }
                    }
                }
            }

            decrypted_messages
        }

        pub fn members(&self, member: &MemberId) -> Vec<MemberId> {
            let y = self.members.get(member).expect("member exists");
            let mut members = Vec::from_iter(MessageGroup::members(y).unwrap());
            members.sort();
            members
        }

        fn get_y(&mut self, member: &MemberId) -> TestGroupState {
            self.members.remove(member).expect("member exists")
        }

        fn set_y(&mut self, y: TestGroupState) {
            assert!(
                self.members.insert(y.my_id, y).is_none(),
                "state was removed before insertion"
            );
        }
    }

    #[test]
    fn it_works() {
        let rng = Rng::from_seed([1; 32]);

        let alice = 0;
        let bob = 1;
        let charlie = 2;

        let mut network = Network::new([alice, bob, charlie], rng);

        // Alice creates a group with Bob.
        network.create(alice, vec![bob]);

        // Everyone processes each other's messages.
        let results = network.process();
        assert!(
            results.is_empty(),
            "no decrypted application messages expected"
        );

        // Alice and Bob share the same members view, Charlie is not member yet.
        for member in [alice, bob] {
            assert_eq!(network.members(&member), vec![alice, bob]);
        }
        assert_eq!(network.members(&charlie), vec![]);
    }
}
