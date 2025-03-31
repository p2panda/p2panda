// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

use crate::crypto::x25519::SecretKey;
use crate::message_scheme::acked_dgm::test_utils::AckedTestDGM;
use crate::message_scheme::{
    AckMessage, AddAckMessage, ControlMessage, Dcgka, DcgkaState, DirectMessageType,
    OperationOutput, ProcessMessage, ProcessOutput, UpdateSecret,
};
use crate::traits::{AckedGroupMembership, PreKeyManager};
use crate::{KeyManager, KeyRegistry, Lifetime, Rng};

pub type MemberId = usize;

pub type MessageId = usize;

pub type TestDcgkaState = DcgkaState<
    MemberId,
    MessageId,
    KeyRegistry<MemberId>,
    AckedTestDGM<MemberId, MessageId>,
    KeyManager,
>;

/// Helper method returning initialised DCGKA state for each member of a test group.
///
/// The method will automatically generate all required one-time pre-key bundles from each member
/// and register them for each other.
pub fn init_dcgka_state<const N: usize>(
    member_ids: [MemberId; N],
    rng: &Rng,
) -> [TestDcgkaState; N] {
    let mut key_bundles = HashMap::new();
    let mut key_managers = HashMap::new();

    // Generate a pre-key bundle for each other member of the group.
    for id in member_ids {
        let identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let mut manager = KeyManager::init(&identity_secret, Lifetime::default(), &rng).unwrap();

        let mut bundle_list = Vec::with_capacity(member_ids.len());
        for _ in member_ids {
            let (manager_i, key_bundle) =
                KeyManager::generate_onetime_bundle(manager, &rng).unwrap();
            bundle_list.push(key_bundle);
            manager = manager_i;
        }

        key_bundles.insert(id, bundle_list);
        key_managers.insert(id, manager);
    }

    // Register each other's pre-key bundles and initialise DCGKA state.
    let mut result = Vec::with_capacity(member_ids.len());
    for id in member_ids {
        let dgm = AckedTestDGM::init(id);
        let registry = {
            let mut state = KeyRegistry::init();
            for bundle_id in member_ids {
                let bundle = key_bundles.get_mut(&bundle_id).unwrap().pop().unwrap();
                let state_i = KeyRegistry::add_onetime_bundle(state, bundle_id, bundle);
                state = state_i;
            }
            state
        };
        let manager = key_managers.remove(&id).unwrap();
        let dcgka: TestDcgkaState = Dcgka::init(id, manager, registry, dgm);
        result.push(dcgka);
    }

    result.try_into().unwrap()
}

fn members_without(members: &[MemberId], without: &[MemberId]) -> Vec<MemberId> {
    members
        .iter()
        .filter(|id| !without.contains(id))
        .cloned()
        .collect()
}

/// Test tool to assert DCGKA group operations and states.
pub struct AssertableDcgka {
    /// Update secrets for "local member -> remote member".
    update_secrets: HashMap<(MemberId, MemberId), UpdateSecret>,
}

impl AssertableDcgka {
    pub fn new() -> Self {
        Self {
            update_secrets: HashMap::new(),
        }
    }

    /// Expected local state after a member created a group.
    pub fn assert_create(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &OperationOutput<MemberId, MessageId, AckedTestDGM<MemberId, MessageId>>,
        creator_id: MemberId,
        expected_members: &[MemberId],
    ) {
        // Creator broadcasts a "Create" control message to everyone.
        let ControlMessage::Create(ref message) = output.control_message else {
            panic!("expected \"create\" control message");
        };
        assert_eq!(
            message.initial_members,
            expected_members.to_vec(),
            "create message should contain all expected initial members"
        );

        // Creator sends direct 2SM messages to each other member of the group.
        assert_eq!(output.direct_messages.len(), expected_members.len() - 1);
        for (index, expected_member) in members_without(expected_members, &[creator_id])
            .iter()
            .enumerate()
        {
            assert_eq!(
                output.direct_messages.get(index).unwrap().message_type(),
                DirectMessageType::TwoParty,
                "create operation should yield a 2SM direct message"
            );
            assert_eq!(
                output.direct_messages.get(index).unwrap().recipient,
                *expected_member,
                "direct message should address expected initial member",
            );
        }

        // Creator establishes the update secret for their own message ratchet.
        assert!(
            output.me_update_secret.is_some(),
            "creator received a new update secret"
        );

        // Creator considers all members part of the group now.
        for expected_member in expected_members {
            assert_eq!(
                AckedTestDGM::members_view(&dcgka.dgm, &expected_member).unwrap(),
                HashSet::from_iter(expected_members.iter().cloned()),
                "creator considers all initial members to be part of their group",
            );
        }

        // Remember creator's update secret for later assertions.
        self.update_secrets.insert(
            (creator_id, creator_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );
    }

    /// Expected local state after an invited member processed a "create" control message.
    pub fn assert_process_create(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &ProcessOutput<MemberId, MessageId, AckedTestDGM<MemberId, MessageId>>,
        processor_id: MemberId,
        creator_id: MemberId,
        expected_members: &[MemberId],
        expected_seq_num: MessageId,
    ) {
        // Processor of "create" message broadcasts an "ack" control message to everyone.
        let Some(ControlMessage::Ack(AckMessage {
            ack_sender,
            ack_seq,
        })) = output.control_message
        else {
            panic!("expected \"ack\" control message");
        };

        // Processor acknowledges the "create" message of the creator.
        assert_eq!(ack_sender, creator_id);
        assert_eq!(ack_seq, expected_seq_num);

        // No direct messages.
        assert!(output.direct_messages.is_empty());

        // Processor establishes the update secret for their own message ratchet.
        assert!(output.me_update_secret.is_some());

        // Processor establishes the update secret for creator's message ratchet.
        assert!(output.sender_update_secret.is_some());

        // Processor of "create" considers all members part of the group now.
        for expected_member in expected_members {
            assert_eq!(
                AckedTestDGM::members_view(&dcgka.dgm, &expected_member).unwrap(),
                HashSet::from_iter(expected_members.iter().cloned()),
                "processor considers all initial members to be part of their group",
            );
        }

        // Remember processor's update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, processor_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );

        // Remember creator's update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, creator_id),
            output.sender_update_secret.as_ref().unwrap().clone(),
        );

        // Processor should be aware now of creator's update secret.
        self.assert_update_secrets(processor_id, creator_id);
    }

    pub fn assert_process_ack(
        &mut self,
        _dcgka: &TestDcgkaState,
        output: &ProcessOutput<MemberId, MessageId, AckedTestDGM<MemberId, MessageId>>,
        processor_id: MemberId,
        acker_id: MemberId,
    ) {
        // No control messages or direct messages.
        assert!(output.control_message.is_none());
        assert!(output.direct_messages.is_empty());

        // No new update secret for acking member.
        assert!(output.me_update_secret.is_none());

        // Processor establishes the update secret for acking member's message ratchet.
        assert!(output.sender_update_secret.is_some());

        // Remember ackers's update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, acker_id),
            output.sender_update_secret.as_ref().unwrap().clone(),
        );

        // Processor should be aware now of acker's update secret.
        self.assert_update_secrets(processor_id, acker_id);
    }

    pub fn assert_add(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &OperationOutput<MemberId, MessageId, AckedTestDGM<MemberId, MessageId>>,
        adder_id: MemberId,
        added_id: MemberId,
        expected_members: &[MemberId],
    ) {
        // Adder broadcasts an "Add" control message to everyone.
        let ControlMessage::Add(ref message) = output.control_message else {
            panic!("expected \"add\" control message");
        };
        assert_eq!(
            message.added, added_id,
            "add message should mention correct added member"
        );

        // One direct message to the added is generated.
        assert_eq!(output.direct_messages.len(), 1);
        assert_eq!(
            output.direct_messages.get(0).unwrap().message_type(),
            DirectMessageType::Welcome
        );
        assert_eq!(output.direct_messages.get(0).unwrap().recipient, added_id);

        // Adder establishes a new update secret for their own message ratchet.
        assert!(output.me_update_secret.is_some());

        // From the perspective of the adder all the other's do not include the added in their
        // member views yet.
        for expected_member in members_without(expected_members, &[adder_id, added_id]) {
            assert_eq!(
                AckedTestDGM::members_view(&dcgka.dgm, &expected_member).unwrap(),
                HashSet::from_iter(members_without(expected_members, &[added_id])),
                "other members do not consider added to be part of group yet",
            );
        }

        // Adder and added consider all members part of the group.
        assert_eq!(
            AckedTestDGM::members_view(&dcgka.dgm, &adder_id).unwrap(),
            HashSet::from_iter(expected_members.iter().cloned()),
            "adder considers all members to be part of their group",
        );
        assert_eq!(
            AckedTestDGM::members_view(&dcgka.dgm, &added_id).unwrap(),
            HashSet::from_iter(expected_members.iter().cloned()),
            "added considers all members to be part of their group",
        );

        // Remember adders's update secret for later assertions.
        let previous = self.update_secrets.insert(
            (adder_id, adder_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );

        // The new update secret does not match the previous one.
        assert_ne!(
            previous.unwrap(),
            output.me_update_secret.as_ref().unwrap().clone(),
        );
    }

    pub fn assert_process_welcome(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &ProcessOutput<MemberId, MessageId, AckedTestDGM<MemberId, MessageId>>,
        adder_id: MemberId,
        added_id: MemberId,
        expected_members: &[MemberId],
        expected_seq_num: MessageId,
    ) {
        // Added broadcasts an "Ack" control message to everyone, no direct messages.
        let Some(ControlMessage::Ack(AckMessage {
            ack_sender,
            ack_seq,
        })) = output.control_message
        else {
            panic!("expected \"ack\" control message");
        };

        // Added acknowledges the "add" message of the adder.
        assert_eq!(ack_sender, adder_id);
        assert_eq!(ack_seq, expected_seq_num);

        // No direct messages.
        assert!(output.direct_messages.is_empty());

        // Added considers all members part of the group now.
        assert_eq!(
            AckedTestDGM::members_view(&dcgka.dgm, &added_id).unwrap(),
            HashSet::from_iter(expected_members.iter().cloned()),
            "added considers all members to be part of their group",
        );

        // Added considers adder to have the same members view.
        assert_eq!(
            AckedTestDGM::members_view(&dcgka.dgm, &adder_id).unwrap(),
            HashSet::from_iter(expected_members.iter().cloned()),
            "adder considers all members to be part of their group",
        );

        // Remember added's update secret for later assertions.
        self.update_secrets.insert(
            (added_id, added_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );

        // Remember adder's update secret for later assertions.
        self.update_secrets.insert(
            (added_id, adder_id),
            output.sender_update_secret.as_ref().unwrap().clone(),
        );

        // Added should be aware now of adder's update secret.
        self.assert_update_secrets(added_id, adder_id);
    }

    pub fn assert_process_add(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &ProcessOutput<MemberId, MessageId, AckedTestDGM<MemberId, MessageId>>,
        processor_id: MemberId,
        adder_id: MemberId,
        added_id: MemberId,
        expected_members: &[MemberId],
        expected_seq_num: MessageId,
    ) {
        // Processor broadcasts an "AddAck" control message to everyone.
        let Some(ControlMessage::AddAck(AddAckMessage {
            ack_sender,
            ack_seq,
        })) = output.control_message
        else {
            panic!("expected \"add-ack\" control message");
        };

        // Processor acknowledges the "add" message of the adder.
        assert_eq!(ack_sender, adder_id);
        assert_eq!(ack_seq, expected_seq_num);

        // Processor forwards a direct message to added. It is required so the added member can
        // decrypt subsequent messages of the processing member.
        assert_eq!(output.direct_messages.len(), 1);
        assert_eq!(output.direct_messages.get(0).unwrap().recipient, added_id);
        assert_eq!(
            output.direct_messages.get(0).unwrap().message_type(),
            DirectMessageType::Forward
        );

        // Expected members view matches.
        assert_eq!(
            AckedTestDGM::members_view(&dcgka.dgm, &processor_id).unwrap(),
            HashSet::from_iter(expected_members.iter().cloned()),
        );

        // Remember processor's update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, processor_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );

        // Remember adder's update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, adder_id),
            output.sender_update_secret.as_ref().unwrap().clone(),
        );

        // Processor should be aware now of adder's update secret.
        self.assert_update_secrets(processor_id, adder_id);
    }

    pub fn assert_process_add_ack(
        &mut self,
        _dcgka: &TestDcgkaState,
        output: &ProcessOutput<MemberId, MessageId, AckedTestDGM<MemberId, MessageId>>,
        processor_id: MemberId,
        add_acker_id: MemberId,
    ) {
        // No control messages and no direct messages.
        assert!(output.control_message.is_none());
        assert!(output.direct_messages.is_empty());

        // No new update secret for processor's own message ratchet.
        assert!(output.me_update_secret.is_none());

        // Processor establishes the update secret for add-acking member's message ratchet.
        assert!(output.sender_update_secret.is_some());

        // Remember ackers's update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, add_acker_id),
            output.sender_update_secret.as_ref().unwrap().clone(),
        );

        // Processor should be aware now of add-acker's update secret.
        self.assert_update_secrets(processor_id, add_acker_id);
    }

    pub fn assert_remove(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &OperationOutput<MemberId, MessageId, AckedTestDGM<MemberId, MessageId>>,
        remover_id: MemberId,
        removed_id: MemberId,
        expected_members: &[MemberId],
    ) {
        // Remover broadcasts a "Remove" control message to everyone.
        let ControlMessage::Remove(ref message) = output.control_message else {
            panic!("expected \"remove\" control message");
        };
        assert_eq!(
            message.removed, removed_id,
            "remove message should mention correct removed member"
        );

        // Remover sends a direct 2SM message to each other member of the group who is left.
        assert_eq!(output.direct_messages.len(), expected_members.len() - 1);
        for (index, expected_member) in members_without(expected_members, &[remover_id, removed_id])
            .iter()
            .enumerate()
        {
            assert_eq!(
                output.direct_messages.get(index).unwrap().message_type(),
                DirectMessageType::TwoParty,
                "remove operation should yield a 2SM direct message"
            );
            assert_eq!(
                output.direct_messages.get(index).unwrap().recipient,
                *expected_member,
                "direct message should address expected member",
            );
        }

        // Remover establishes a new update secret for their own message ratchet.
        assert!(output.me_update_secret.is_some());

        // Remover has correct member view.
        assert_eq!(
            AckedTestDGM::members_view(&dcgka.dgm, &remover_id).unwrap(),
            HashSet::from_iter(expected_members.iter().cloned()),
        );

        // Remember remover's update secret for later assertions.
        self.update_secrets.insert(
            (remover_id, remover_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );
    }

    pub fn assert_process_remove(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &ProcessOutput<MemberId, MessageId, AckedTestDGM<MemberId, MessageId>>,
        processor_id: MemberId,
        remover_id: MemberId,
        expected_members: &[MemberId],
        expected_seq_num: MessageId,
    ) {
        // Processor of "remove" message broadcasts an "ack" control message to everyone.
        let Some(ControlMessage::Ack(AckMessage {
            ack_sender,
            ack_seq,
        })) = output.control_message
        else {
            panic!("expected \"ack\" control message");
        };

        // Processor acknowledges the "remove" message of the remover.
        assert_eq!(ack_sender, remover_id);
        assert_eq!(ack_seq, expected_seq_num);

        // No direct messages.
        assert!(output.direct_messages.is_empty());

        // Processor establishes the update secret for their own message ratchet.
        assert!(output.me_update_secret.is_some());

        // Processor establishes the update secret for remover's message ratchet.
        assert!(output.sender_update_secret.is_some());

        // Processor of "remove" has expected members view
        assert_eq!(
            AckedTestDGM::members_view(&dcgka.dgm, &processor_id).unwrap(),
            HashSet::from_iter(expected_members.iter().cloned()),
        );

        // Remember processor's update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, processor_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );

        // Remember remover's update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, remover_id),
            output.sender_update_secret.as_ref().unwrap().clone(),
        );

        // Processor should be aware now of remover's update secret.
        self.assert_update_secrets(processor_id, remover_id);
    }

    pub fn assert_update(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &OperationOutput<MemberId, MessageId, AckedTestDGM<MemberId, MessageId>>,
        updater_id: MemberId,
        expected_members: &[MemberId],
    ) {
        // Updater broadcasts an "Update" control message to everyone.
        let ControlMessage::Update(_) = output.control_message else {
            panic!("expected \"update\" control message");
        };

        // Updater sends a direct 2SM message to each other member of the group.
        assert_eq!(output.direct_messages.len(), expected_members.len() - 1);
        for (index, expected_member) in members_without(expected_members, &[updater_id])
            .iter()
            .enumerate()
        {
            assert_eq!(
                output.direct_messages.get(index).unwrap().message_type(),
                DirectMessageType::TwoParty,
                "update operation should yield a 2SM direct message"
            );
            assert_eq!(
                output.direct_messages.get(index).unwrap().recipient,
                *expected_member,
                "direct message should address expected member",
            );
        }

        // Updater establishes a new update secret for their own message ratchet.
        assert!(output.me_update_secret.is_some());

        // Updater has correct member view.
        assert_eq!(
            AckedTestDGM::members_view(&dcgka.dgm, &updater_id).unwrap(),
            HashSet::from_iter(expected_members.iter().cloned()),
        );

        // Remember updater's update secret for later assertions.
        self.update_secrets.insert(
            (updater_id, updater_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );
    }

    pub fn assert_process_update(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &ProcessOutput<MemberId, MessageId, AckedTestDGM<MemberId, MessageId>>,
        processor_id: MemberId,
        updater_id: MemberId,
        expected_members: &[MemberId],
        expected_seq_num: MessageId,
    ) {
        // Processor of "update" message broadcasts an "ack" control message to everyone.
        let Some(ControlMessage::Ack(AckMessage {
            ack_sender,
            ack_seq,
        })) = output.control_message
        else {
            panic!("expected \"ack\" control message");
        };

        // Processor acknowledges the "update" message of the updater.
        assert_eq!(ack_sender, updater_id);
        assert_eq!(ack_seq, expected_seq_num);

        // No direct messages.
        assert!(output.direct_messages.is_empty());

        // Processor establishes the update secret for their own message ratchet.
        assert!(output.me_update_secret.is_some());

        // Processor establishes the update secret for remover's message ratchet.
        assert!(output.sender_update_secret.is_some());

        // Processor of "update" has expected members view
        assert_eq!(
            AckedTestDGM::members_view(&dcgka.dgm, &processor_id).unwrap(),
            HashSet::from_iter(expected_members.iter().cloned()),
        );

        // Remember processor's update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, processor_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );

        // Remember updater's update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, updater_id),
            output.sender_update_secret.as_ref().unwrap().clone(),
        );

        // Processor should be aware now of updater's update secret.
        self.assert_update_secrets(processor_id, updater_id);
    }

    /// Compare if member learned about the update secret from another member.
    fn assert_update_secrets(&self, from: MemberId, to: MemberId) {
        assert_eq!(
            self.update_secrets.get(&(from, to)).unwrap().as_bytes(),
            self.update_secrets.get(&(to, to)).unwrap().as_bytes(),
        );
    }
}

impl<ID, OP, DGM> TryFrom<(&OperationOutput<ID, OP, DGM>, Option<ID>)>
    for ProcessMessage<ID, OP, DGM>
where
    DGM: Clone + AckedGroupMembership<ID, OP>,
    ID: Clone + Debug + PartialEq,
    OP: Clone,
{
    type Error = String;

    fn try_from(args: (&OperationOutput<ID, OP, DGM>, Option<ID>)) -> Result<Self, Self::Error> {
        let (output, dm_recipient) = args;
        let direct_message = match dm_recipient {
            Some(ref dm) => output
                .direct_messages
                .iter()
                .find(|message| &message.recipient == dm)
                .cloned(),
            None => None,
        };

        if dm_recipient.is_some() && direct_message.is_none() {
            return Err(format!(
                "expected direct message for user {:?}",
                dm_recipient.unwrap()
            ));
        }

        Ok(match &output.control_message {
            ControlMessage::Create(create_message) => {
                ProcessMessage::Create(create_message.clone(), direct_message.unwrap())
            }
            ControlMessage::Ack(ack_message) => {
                ProcessMessage::Ack(ack_message.clone(), direct_message)
            }
            ControlMessage::Update(update_message) => {
                ProcessMessage::Update(update_message.clone(), direct_message.unwrap())
            }
            ControlMessage::Remove(remove_message) => {
                ProcessMessage::Remove(remove_message.clone(), direct_message.unwrap())
            }
            ControlMessage::Add(add_message) => {
                ProcessMessage::Add(add_message.clone(), direct_message)
            }
            ControlMessage::AddAck(add_ack_message) => {
                ProcessMessage::AddAck(add_ack_message.clone(), direct_message)
            }
        })
    }
}

impl<ID, OP, DGM> TryFrom<(&ProcessOutput<ID, OP, DGM>, Option<ID>)> for ProcessMessage<ID, OP, DGM>
where
    DGM: Clone + AckedGroupMembership<ID, OP>,
    ID: Clone + Debug + PartialEq,
    OP: Clone,
{
    type Error = String;

    fn try_from(args: (&ProcessOutput<ID, OP, DGM>, Option<ID>)) -> Result<Self, Self::Error> {
        let (output, dm_recipient) = args;
        let direct_message = match dm_recipient {
            Some(ref dm) => output
                .direct_messages
                .iter()
                .find(|message| &message.recipient == dm)
                .cloned(),
            None => None,
        };

        if dm_recipient.is_some() && direct_message.is_none() {
            return Err(format!(
                "expected direct message for user {:?}",
                dm_recipient.unwrap()
            ));
        }

        Ok(match output.control_message.as_ref().unwrap() {
            ControlMessage::Create(create_message) => {
                ProcessMessage::Create(create_message.clone(), direct_message.unwrap())
            }
            ControlMessage::Ack(ack_message) => {
                ProcessMessage::Ack(ack_message.clone(), direct_message)
            }
            ControlMessage::Update(update_message) => {
                ProcessMessage::Update(update_message.clone(), direct_message.unwrap())
            }
            ControlMessage::Remove(remove_message) => {
                ProcessMessage::Remove(remove_message.clone(), direct_message.unwrap())
            }
            ControlMessage::Add(add_message) => {
                ProcessMessage::Add(add_message.clone(), direct_message)
            }
            ControlMessage::AddAck(add_ack_message) => {
                ProcessMessage::AddAck(add_ack_message.clone(), direct_message)
            }
        })
    }
}
