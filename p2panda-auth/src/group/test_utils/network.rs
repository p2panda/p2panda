// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, VecDeque};

use crate::group::{GroupAction, GroupControlMessage, GroupMember, access::Access};
use crate::traits::{AuthGraph, Operation, Ordering};

use super::{
    TestGroup, TestGroupState, TestGroupStateInner, TestGroupStoreState, TestOperation,
    TestOrderer, TestOrdererState,
};

type MemberId = char;
type GroupId = char;
type MessageId = u32;

pub struct Network {
    group_store_y: TestGroupStoreState<MemberId, TestGroupStateInner>,
    members: HashMap<MemberId, HashMap<GroupId, TestGroupState>>,
    queue: VecDeque<TestOperation<MemberId, MessageId>>,
}

impl Network {
    pub fn new<const N: usize>(members: [MemberId; N]) -> Self {
        Self {
            group_store_y: TestGroupStoreState::default(),
            members: HashMap::from_iter(
                members
                    .into_iter()
                    .map(|member_id| (member_id, HashMap::default())),
            ),
            queue: VecDeque::new(),
        }
    }

    pub fn create(
        &mut self,
        group_id: GroupId,
        creator: MemberId,
        initial_members: Vec<(GroupMember<MemberId>, Access)>,
    ) {
        let y = TestGroupState::new(
            creator,
            group_id,
            self.group_store_y.clone(),
            TestOrdererState::new(creator),
        );
        let control_message = GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Create { initial_members },
        };
        let (y_i, operation) = TestGroup::prepare(y, &control_message).unwrap();
        let y_ii = TestGroup::process(y_i, &operation).unwrap();
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
        let y_ii = TestGroup::process(y_i, &operation).unwrap();
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

                    y = TestGroup::process(y.clone(), &operation).unwrap();
                }
                self.set_y(y);
            }
        }
    }

    pub fn members(
        &self,
        member: &MemberId,
        group_id: &GroupId,
    ) -> Vec<(GroupMember<MemberId>, Access)> {
        let y = self
            .members
            .get(member)
            .expect("member exists")
            .get(group_id)
            .expect("group exists");

        let mut members = y.members();
        members.sort();
        members
    }

    fn get_y(&mut self, member: &MemberId, group_id: &GroupId) -> TestGroupState {
        let group_y = self
            .members
            .get_mut(member)
            .expect("member exists")
            .remove(group_id);

        match group_y {
            Some(group_y) => group_y,
            None => TestGroupState::new(
                *member,
                *group_id,
                self.group_store_y.clone(),
                TestOrdererState::new(*member),
            ),
        }
    }

    fn set_y(&mut self, y: TestGroupState) {
        assert!(
            self.members
                .get_mut(&y.my_id)
                .expect("member exists")
                .insert(y.id(), y)
                .is_none(),
            "state was removed before insertion"
        );
    }
}
