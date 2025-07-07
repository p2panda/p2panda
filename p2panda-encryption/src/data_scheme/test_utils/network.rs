// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet, VecDeque};

use crate::crypto::Rng;
use crate::data_scheme::ControlMessage;
use crate::data_scheme::group::{EncryptionGroup, GroupOutput, GroupState};
use crate::data_scheme::group_secret::SecretBundle;
use crate::data_scheme::test_utils::dcgka::init_dcgka_state;
use crate::data_scheme::test_utils::dgm::TestDgm;
use crate::data_scheme::test_utils::ordering::{MessageOrderer, TestMessage};
use crate::key_manager::KeyManager;
use crate::key_registry::KeyRegistry;
use crate::test_utils::{MemberId, MessageId};
use crate::traits::{GroupMessage, GroupMessageContent};

pub type TestGroupState = GroupState<
    MemberId,
    MessageId,
    KeyRegistry<MemberId>,
    TestDgm<MemberId, MessageId>,
    KeyManager,
    MessageOrderer<TestDgm<MemberId, MessageId>>,
>;

pub fn init_group_state<const N: usize>(
    member_ids: [MemberId; N],
    rng: &Rng,
) -> [TestGroupState; N] {
    init_dcgka_state(member_ids, rng)
        .into_iter()
        .map(|dcgka| {
            let orderer = MessageOrderer::<TestDgm<MemberId, MessageId>>::init(dcgka.my_id);
            TestGroupState {
                my_id: dcgka.my_id,
                dcgka,
                orderer,
                secrets: SecretBundle::init(),
                is_welcomed: false,
            }
        })
        .collect::<Vec<TestGroupState>>()
        .try_into()
        .unwrap()
}

pub struct Network {
    rng: Rng,
    pub members: HashMap<MemberId, TestGroupState>,
    pub removed_members: HashSet<MemberId>,
    pub queue: VecDeque<TestMessage<TestDgm<MemberId, MessageId>>>,
}

impl Network {
    pub fn new<const N: usize>(members: [MemberId; N], rng: Rng) -> Self {
        let members = init_group_state(members, &rng);
        Self {
            rng,
            members: HashMap::from_iter(members.into_iter().map(|state| (state.my_id, state))),
            removed_members: HashSet::new(),
            queue: VecDeque::new(),
        }
    }

    pub fn create(&mut self, creator: MemberId, initial_members: Vec<MemberId>) {
        let y = self.get_y(&creator);
        let (y_i, message) = EncryptionGroup::create(y, initial_members, &self.rng).unwrap();
        self.queue.push_back(message);
        self.set_y(y_i);
    }

    pub fn add(&mut self, adder: MemberId, added: MemberId) {
        let y = self.get_y(&adder);
        let (y_i, message) = EncryptionGroup::add(y, added, &self.rng).unwrap();
        self.queue.push_back(message);
        self.set_y(y_i);
    }

    pub fn remove(&mut self, remover: MemberId, removed: MemberId) {
        let y = self.get_y(&remover);
        let (y_i, message) = EncryptionGroup::remove(y, removed, &self.rng).unwrap();
        self.queue.push_back(message);
        self.set_y(y_i);
        self.removed_members.insert(removed);
    }

    pub fn update(&mut self, updater: MemberId) {
        let y = self.get_y(&updater);
        let (y_i, message) = EncryptionGroup::update(y, &self.rng).unwrap();
        self.queue.push_back(message);
        self.set_y(y_i);
    }

    pub fn send(&mut self, sender: MemberId, plaintext: &[u8]) {
        let y = self.get_y(&sender);
        let (y_i, message) = EncryptionGroup::send(y, plaintext, &self.rng).unwrap();
        self.queue.push_back(message);
        self.set_y(y_i);
    }

    pub fn process(&mut self) -> Vec<(MemberId, MemberId, Vec<u8>)> {
        if self.queue.is_empty() {
            return Vec::new();
        }

        let mut decrypted_messages = Vec::new();
        let member_ids: Vec<MemberId> = self
            .members
            .keys()
            .cloned()
            .filter(|id| !self.removed_members.contains(id))
            .collect();

        while let Some(message) = self.queue.pop_front() {
            for id in &member_ids {
                // Do not process our own messages.
                if &message.sender() == id {
                    continue;
                }

                // Member processes each message broadcast to the group.
                let y = self.get_y(id);
                let (y_i, result) = EncryptionGroup::receive(y, &message).unwrap();
                self.set_y(y_i);

                for output in result {
                    match output {
                        GroupOutput::Control(control_message) => {
                            // Processing messages might yield new ones, process these as well.
                            self.queue.push_back(control_message);
                        }
                        GroupOutput::Application { plaintext } => decrypted_messages.push((
                            message.sender(), // Sender
                            *id,              // Receiver
                            plaintext,        // Decrypted content
                        )),
                        GroupOutput::Removed => (),
                    }
                }

                // Update set of removed members if any.
                if let GroupMessageContent::Control(ControlMessage::Remove { removed }) =
                    message.content()
                {
                    self.removed_members.insert(removed);
                }
            }
        }

        decrypted_messages.sort();
        decrypted_messages
    }

    pub fn members(&self, member: &MemberId) -> Vec<MemberId> {
        let y = self.members.get(member).expect("member exists");
        let mut members = Vec::from_iter(EncryptionGroup::members(y).unwrap());
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
