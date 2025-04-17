// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, VecDeque};

use crate::message_scheme::GroupError;
use crate::message_scheme::acked_dgm::test_utils::AckedTestDgm;
use crate::message_scheme::group::{GroupConfig, GroupOutput, GroupState, MessageGroup};
use crate::message_scheme::test_utils::dcgka::init_dcgka_state;
use crate::message_scheme::test_utils::ordering::{ForwardSecureOrderer, TestMessage};
use crate::test_utils::{MemberId, MessageId};
use crate::traits::ForwardSecureGroupMessage;
use crate::{KeyManager, KeyRegistry, Rng};

pub type TestGroupState = GroupState<
    MemberId,
    MessageId,
    KeyRegistry<MemberId>,
    AckedTestDgm<MemberId, MessageId>,
    KeyManager,
    ForwardSecureOrderer<AckedTestDgm<MemberId, MessageId>>,
>;

pub type TestGroupError = GroupError<
    MemberId,
    MessageId,
    KeyRegistry<MemberId>,
    AckedTestDgm<MemberId, MessageId>,
    KeyManager,
    ForwardSecureOrderer<AckedTestDgm<MemberId, MessageId>>,
>;

pub fn init_group_state<const N: usize>(
    member_ids: [MemberId; N],
    rng: &Rng,
) -> [TestGroupState; N] {
    init_dcgka_state(member_ids, rng)
        .into_iter()
        .map(|dcgka| {
            let orderer =
                ForwardSecureOrderer::<AckedTestDgm<MemberId, MessageId>>::init(dcgka.my_id);
            TestGroupState {
                my_id: dcgka.my_id,
                dcgka,
                orderer,
                ratchet: None,
                decryption_ratchet: HashMap::new(),
                config: GroupConfig::default(),
            }
        })
        .collect::<Vec<TestGroupState>>()
        .try_into()
        .unwrap()
}

pub struct Network {
    rng: Rng,
    members: HashMap<MemberId, TestGroupState>,
    queue: VecDeque<TestMessage<AckedTestDgm<MemberId, MessageId>>>,
}

impl Network {
    pub fn new<const N: usize>(members: [MemberId; N], rng: Rng) -> Self {
        let members = init_group_state(members, &rng);
        Self {
            members: HashMap::from_iter(members.into_iter().map(|state| (state.my_id, state))),
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
        self.get_y(&removed);
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

    pub fn process(&mut self) -> Vec<(MemberId, MemberId, Vec<u8>)> {
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

                // Member processes each message broadcast to the group.
                let y = self.get_y(id);
                let (y_i, result) = MessageGroup::receive(y, &message, &self.rng).unwrap();
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
            }
        }

        decrypted_messages.sort();
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
