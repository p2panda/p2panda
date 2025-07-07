// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet, VecDeque};

use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;

use crate::Access;
use crate::group::{GroupAction, GroupControlMessage, GroupMember};
use crate::traits::{GroupStore, Operation, Orderer};

use super::{
    MemberId, MessageId, TestGroup, TestGroupState, TestGroupStore, TestOperation, TestOrderer,
    TestOrdererState,
};

pub struct Network {
    members: HashMap<MemberId, NetworkMember>,
    queue: VecDeque<TestOperation>,
    rng: StdRng,
}

pub struct NetworkMember {
    id: MemberId,
    group_store: TestGroupStore,
    orderer_y: TestOrdererState,
}

impl Network {
    pub fn new<const N: usize>(members: [MemberId; N], mut rng: StdRng) -> Self {
        Self {
            members: HashMap::from_iter(members.into_iter().map(|member_id| {
                let group_store = TestGroupStore::default();
                let orderer_y = TestOrdererState::new(
                    member_id,
                    group_store.clone(),
                    StdRng::from_rng(&mut rng),
                );
                (
                    member_id,
                    NetworkMember {
                        id: member_id,
                        group_store,
                        orderer_y,
                    },
                )
            })),
            queue: VecDeque::new(),
            rng,
        }
    }

    pub fn create(
        &mut self,
        group_id: MemberId,
        creator: MemberId,
        initial_members: Vec<(GroupMember<MemberId>, Access<()>)>,
    ) -> MessageId {
        let y = self.get_y(&creator, &group_id);
        let control_message = GroupControlMessage {
            group_id,
            action: GroupAction::Create { initial_members },
        };
        let (y_i, operation) = TestGroup::prepare(y, &control_message).unwrap();
        let operation_id = operation.id();
        let y_ii = TestGroup::process(y_i, &operation).unwrap();
        self.queue.push_back(operation);
        self.set_y(y_ii);
        operation_id
    }

    pub fn add(
        &mut self,
        adder: MemberId,
        added: GroupMember<MemberId>,
        group_id: MemberId,
        access: Access<()>,
    ) -> MessageId {
        let y = self.get_y(&adder, &group_id);
        let control_message = GroupControlMessage {
            group_id,
            action: GroupAction::Add {
                member: added,
                access,
            },
        };
        let (y_i, operation) = TestGroup::prepare(y, &control_message).unwrap();
        let y_ii = TestGroup::process(y_i, &operation).unwrap();
        let operation_id = operation.id();
        self.queue.push_back(operation);
        self.set_y(y_ii);
        operation_id
    }

    pub fn remove(
        &mut self,
        remover: MemberId,
        removed: GroupMember<MemberId>,
        group_id: MemberId,
    ) -> MessageId {
        let y = self.get_y(&remover, &group_id);
        let control_message = GroupControlMessage {
            group_id,
            action: GroupAction::Remove { member: removed },
        };
        let (y_i, operation) = TestGroup::prepare(y, &control_message).unwrap();
        let y_ii = TestGroup::process(y_i, &operation).unwrap();
        let operation_id = operation.id();
        self.queue.push_back(operation);
        self.set_y(y_ii);
        operation_id
    }

    pub fn demote(
        &mut self,
        demoter: MemberId,
        demoted: GroupMember<MemberId>,
        group_id: MemberId,
        access: Access<()>,
    ) -> MessageId {
        let y = self.get_y(&demoter, &group_id);
        let control_message = GroupControlMessage {
            group_id,
            action: GroupAction::Demote {
                member: demoted,
                access,
            },
        };
        let (y_i, operation) = TestGroup::prepare(y, &control_message).unwrap();
        let y_ii = TestGroup::process(y_i, &operation).unwrap();
        let operation_id = operation.id();
        self.queue.push_back(operation);
        self.set_y(y_ii);
        operation_id
    }

    pub fn promote(
        &mut self,
        promoter: MemberId,
        promoted: GroupMember<MemberId>,
        group_id: MemberId,
        access: Access<()>,
    ) -> MessageId {
        let y = self.get_y(&promoter, &group_id);
        let control_message = GroupControlMessage {
            group_id,
            action: GroupAction::Promote {
                member: promoted,
                access,
            },
        };
        let (y_i, operation) = TestGroup::prepare(y, &control_message).unwrap();
        let y_ii = TestGroup::process(y_i, &operation).unwrap();
        let operation_id = operation.id();
        self.queue.push_back(operation);
        self.set_y(y_ii);
        operation_id
    }

    pub fn process_ooo(&mut self) {
        if self.queue.is_empty() {
            return;
        }

        let member_ids: Vec<MemberId> = self.members.keys().cloned().collect();

        self.shuffle();
        while let Some(operation) = self.queue.pop_front() {
            for id in &member_ids {
                // Shuffle messages in the queue for each member.
                self.shuffle();
                self.member_process(id, &operation)
            }
        }
    }

    pub fn process(&mut self) {
        if self.queue.is_empty() {
            return;
        }

        let member_ids: Vec<MemberId> = self.members.keys().cloned().collect();

        while let Some(operation) = self.queue.pop_front() {
            for id in &member_ids {
                self.member_process(id, &operation)
            }
        }
    }

    fn member_process(&mut self, member_id: &MemberId, operation: &TestOperation) {
        // Do not process our own messages.
        if &operation.author() == member_id {
            return;
        }

        let control_message = operation.payload();
        let mut group_id = control_message.group_id();
        let mut y = self.get_y(member_id, &group_id);
        let orderer_y = TestOrderer::queue(y.orderer_y.clone(), operation).unwrap();

        loop {
            let (orderer_y, result) = TestOrderer::next_ready_message(orderer_y.clone()).unwrap();
            y.orderer_y = orderer_y;
            self.set_y(y.clone());

            let Some(operation) = result else {
                break;
            };

            if &operation.author() == member_id {
                continue;
            }

            group_id = operation.payload().group_id();
            y = self.get_y(member_id, &group_id);
            y = TestGroup::process(y.clone(), &operation).unwrap();
            self.set_y(y.clone());
        }
    }

    pub fn members(
        &self,
        member: &MemberId,
        group_id: &MemberId,
    ) -> Vec<(GroupMember<MemberId>, Access<()>)> {
        let group_y = self.get_y(member, group_id);
        let mut members = group_y.members();
        members.sort();
        members
    }

    pub fn members_at(
        &self,
        member: &MemberId,
        group_id: &MemberId,
        dependencies: &[MessageId],
    ) -> Vec<(GroupMember<MemberId>, Access<()>)> {
        let group_y = self.get_y(member, group_id);
        let mut members = group_y.members_at(&dependencies.iter().copied().collect::<HashSet<_>>());
        members.sort();
        members
    }

    pub fn transitive_members(
        &self,
        member: &MemberId,
        group_id: &MemberId,
    ) -> Vec<(MemberId, Access<()>)> {
        let group_y = self.get_y(member, group_id);
        let mut members = group_y
            .transitive_members()
            .expect("get transitive members");
        members.sort();
        members
    }

    pub fn transitive_members_at(
        &self,
        member: &MemberId,
        group_id: &MemberId,
        dependencies: &[MessageId],
    ) -> Vec<(MemberId, Access<()>)> {
        let group_y = self.get_y(member, group_id);
        let mut members = group_y
            .transitive_members_at(&dependencies.iter().copied().collect::<HashSet<_>>())
            .expect("get transitive members");
        members.sort();
        members
    }

    fn shuffle(&mut self) {
        let mut queue = self.queue.clone().into_iter().collect::<Vec<_>>();
        queue.shuffle(&mut self.rng);
        self.queue = VecDeque::from(queue);
    }

    pub fn get_y(&self, member: &MemberId, group_id: &MemberId) -> TestGroupState {
        let member = self.members.get(member).expect("member exists");
        let group_y = TestGroupStore::get(&member.group_store, group_id).unwrap();

        match group_y {
            Some(group_y) => group_y,
            None => TestGroupState::new(
                member.id,
                *group_id,
                member.group_store.clone(),
                member.orderer_y.clone(),
            ),
        }
    }

    fn set_y(&mut self, y: TestGroupState) {
        let member = self.members.get_mut(&y.my_id).expect("member exists");
        member.group_store.insert(&y.id(), &y).unwrap();
    }
}
