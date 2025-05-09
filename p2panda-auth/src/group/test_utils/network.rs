// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, VecDeque};

use crate::group::{GroupAction, GroupControlMessage, GroupMember, access::Access};
use crate::traits::{AuthGraph, GroupStore, Operation, Ordering};

use super::{
    GroupId, MemberId, MessageId, TestGroup, TestGroupState, TestGroupStateInner, TestGroupStore,
    TestGroupStoreState, TestOperation, TestOrderer, TestOrdererState,
};

pub struct Network {
    members: HashMap<MemberId, NetworkMember>,
    queue: VecDeque<TestOperation<MemberId, MessageId>>,
}

pub struct NetworkMember {
    id: MemberId,
    group_store_y: TestGroupStoreState<MemberId, TestGroupStateInner>,
    orderer_y: TestOrdererState,
}

impl Network {
    pub fn new<const N: usize>(members: [MemberId; N]) -> Self {
        Self {
            members: HashMap::from_iter(members.into_iter().map(|member_id| {
                let group_store_y = TestGroupStoreState::default();
                let orderer_y = TestOrdererState::new(member_id, group_store_y.clone());
                (
                    member_id,
                    NetworkMember {
                        id: member_id,
                        group_store_y,
                        orderer_y,
                    },
                )
            })),
            queue: VecDeque::new(),
        }
    }

    pub fn create(
        &mut self,
        group_id: GroupId,
        creator: MemberId,
        initial_members: Vec<(GroupMember<MemberId>, Access)>,
    ) {
        let y = self.get_y(&creator, &group_id);

        let control_message = GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Create { initial_members },
        };
        let (y_i, operation) = TestGroup::prepare(y, &control_message).unwrap();
        let mut y_ii = TestGroup::process(y_i, &operation).unwrap();
        y_ii.group_store_y =
            TestGroupStore::insert(y_ii.group_store_y.clone(), &group_id, &y_ii.inner).unwrap();
        self.queue.push_back(operation);
        self.set_y(y_ii);
    }

    pub fn add(
        &mut self,
        adder: MemberId,
        added: GroupMember<MemberId>,
        group_id: GroupId,
        access: Access,
    ) {
        let y = self.get_y(&adder, &group_id);
        let control_message = GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Add {
                member: added,
                access,
            },
        };
        let (y_i, operation) = TestGroup::prepare(y, &control_message).unwrap();
        let mut y_ii = TestGroup::process(y_i, &operation).unwrap();
        y_ii.group_store_y =
            TestGroupStore::insert(y_ii.group_store_y.clone(), &group_id, &y_ii.inner).unwrap();
        self.queue.push_back(operation);
        self.set_y(y_ii);
    }

    pub fn remove(&mut self, remover: MemberId, removed: GroupMember<MemberId>, group_id: GroupId) {
        let y = self.get_y(&remover, &group_id);
        let control_message = GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Remove { member: removed },
        };
        let (y_i, operation) = TestGroup::prepare(y, &control_message).unwrap();
        let y_ii = TestGroup::process(y_i, &operation).unwrap();
        self.queue.push_back(operation);
        self.set_y(y_ii);
    }

    pub fn process(&mut self) {
        if self.queue.is_empty() {
            return;
        }

        let member_ids: Vec<MemberId> = self.members.keys().cloned().collect();

        while let Some(operation) = self.queue.pop_front() {
            let control_message = operation.payload();
            let group_id = control_message.group_id();

            for id in &member_ids {
                // Do not process our own messages.
                if &operation.sender() == id {
                    continue;
                }

                let mut y = self.get_y(id, &group_id);
                y.orderer_y = TestOrderer::queue(y.orderer_y.clone(), &operation).unwrap();

                loop {
                    let (y_orderer_next, result) =
                        TestOrderer::next_ready_message(y.orderer_y.clone()).unwrap();
                    y.orderer_y = y_orderer_next;

                    let Some(message) = result else {
                        break;
                    };
                    y = TestGroup::process(y.clone(), &message).unwrap();
                    y.group_store_y =
                        TestGroupStore::insert(y.group_store_y.clone(), &group_id, &y.inner)
                            .unwrap();
                }
                self.set_y(y.clone());
            }
        }
    }

    pub fn members(
        &self,
        member: &MemberId,
        group_id: &GroupId,
    ) -> Vec<(GroupMember<MemberId>, Access)> {
        let member = self.members.get(member).expect("member exists");

        let group_y_inner = TestGroupStore::get(&member.group_store_y, group_id)
            .unwrap()
            .expect("group exists");

        let mut group_y = TestGroupState::new(
            member.id,
            *group_id,
            member.group_store_y.clone(),
            TestOrdererState::new(member.id, member.group_store_y.clone()),
        );

        group_y = TestGroupState::new_from_inner(&group_y, group_y_inner);

        let mut members = group_y.members();
        members.sort();
        members
    }

    fn get_y(&mut self, member: &MemberId, group_id: &GroupId) -> TestGroupState {
        let member = self.members.get(member).expect("member exists");

        let group_y = TestGroupState::new(
            member.id,
            *group_id,
            member.group_store_y.clone(),
            member.orderer_y.clone(),
        );

        let group_y_inner = TestGroupStore::get(&member.group_store_y, group_id).unwrap();

        match group_y_inner {
            Some(group_y_inner) => TestGroupState::new_from_inner(&group_y, group_y_inner),
            None => group_y,
        }
    }

    fn set_y(&mut self, y: TestGroupState) {
        let member = self.members.get_mut(&y.my_id).expect("member exists");

        let group_store_y =
            TestGroupStore::insert(member.group_store_y.clone(), &y.id(), &y.inner).unwrap();
        member.group_store_y = group_store_y;
    }
}
