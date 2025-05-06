// // SPDX-License-Identifier: MIT OR Apache-2.0
//
// use std::collections::{HashMap, VecDeque};
//
// use crate::{
//     group::{
//         Group, GroupAction, GroupControlMessage, GroupMember, GroupState, access::Access,
//         resolver::GroupResolver,
//     },
//     traits::{AuthGraph, IdentityHandle, OperationId},
// };
//
// use super::{TestOperation, TestOrderer, TestOrdererState};
//
// type MemberId = char;
// type MessageId = u32;
// impl IdentityHandle for MemberId {}
// impl OperationId for MessageId {}
//
// type TestResolver = GroupResolver<MemberId, MessageId, TestOperation<MemberId, MessageId>>;
// type TestGroup = Group<MemberId, MessageId, TestResolver, TestOrderer>;
// type TestGroupState = GroupState<MemberId, MessageId, TestOrderer>;
//
// pub struct Network {
//     members: HashMap<MemberId, TestGroupState>,
//     queue: VecDeque<TestOperation<MemberId, MessageId>>,
// }
//
// impl Network {
//     pub fn new<const N: usize>(group_id: MemberId, members: [MemberId; N]) -> Self {
//         Self {
//             members: HashMap::from_iter(members.into_iter().map(|member_id| {
//                 (member_id, {
//                     TestGroupState::new(member_id, group_id, TestOrdererState::new(member_id))
//                 })
//             })),
//             queue: VecDeque::new(),
//         }
//     }
//
//     pub fn create(
//         &mut self,
//         creator: MemberId,
//         initial_members: Vec<(GroupMember<MemberId>, Access)>,
//     ) {
//         let y = self.get_y(&creator);
//         let control_message = GroupControlMessage::GroupAction {
//             group_id,
//             action: GroupAction::Create { initial_members },
//         };
//         let (y_i, operation) = TestGroup::prepare(y, &control_message).unwrap();
//         let y_ii = TestGroup::process(y_i, &operation).unwrap();
//         self.queue.push_back(operation);
//         self.set_y(y_ii);
//     }
//
//     pub fn add(&mut self, adder: MemberId, added: MemberId, access: Access) {
//         let y = self.get_y(&adder);
//         let operation = GroupOperation::GroupAction(GroupAction::Add {
//             member: added,
//             access,
//         });
//         let (y_i, message) = TestGroup::prepare(y, operation).unwrap();
//         let filter = y_i.filter.clone();
//         let (mut y_ii, filter) = TestGroup::process(y_i, message.clone(), filter).unwrap();
//         y_ii.filter = filter;
//         self.queue.push_back(message);
//         self.set_y(y_ii);
//     }
//
//     pub fn remove(&mut self, remover: MemberId, removed: MemberId) {
//         let y = self.get_y(&remover);
//         let operation = GroupOperation::GroupAction(GroupAction::Remove { member: removed });
//         let (y_i, message) = TestGroup::prepare(y, operation).unwrap();
//         let filter = y_i.filter.clone();
//         let (mut y_ii, filter) = TestGroup::process(y_i, message.clone(), filter).unwrap();
//         y_ii.filter = filter;
//         self.queue.push_back(message);
//         self.set_y(y_ii);
//         self.get_y(&removed);
//     }
//
//     pub fn process(&mut self) {
//         if self.queue.is_empty() {
//             return;
//         }
//
//         let member_ids: Vec<MemberId> = self.members.keys().cloned().collect();
//
//         while let Some(message) = self.queue.pop_front() {
//             for id in &member_ids {
//                 // Do not process our own messages.
//                 if &message.sender() == id {
//                     continue;
//                 }
//
//                 let mut y = self.get_y(id);
//                 let mut filter: GroupFilter<char, u32, Message<char, u32>> = y.filter.clone();
//
//                 y.ordering_y = TestOrderer::queue(y.ordering_y.clone(), &message).unwrap();
//
//                 loop {
//                     let (y_orderer_next, result) =
//                         TestOrderer::next_ready_message(y.ordering_y.clone()).unwrap();
//                     y.ordering_y = y_orderer_next;
//
//                     let Some(message) = result else {
//                         break;
//                     };
//
//                     (y, filter) = TestGroup::process(y.clone(), message, filter).unwrap();
//                 }
//                 y.filter = filter;
//                 self.set_y(y);
//             }
//         }
//     }
//
//     pub fn members(&self, member: &MemberId) -> Vec<(MemberId, Access)> {
//         let y = self.members.get(member).expect("member exists");
//         let mut members = Vec::from_iter(TestGroup::members(y));
//         members.sort();
//         members
//     }
//
//     fn get_y(&mut self, member: &MemberId) -> TestGroupState {
//         self.members.remove(member).expect("member exists")
//     }
//
//     fn set_y(&mut self, y: TestGroupState) {
//         assert!(
//             self.members.insert(y.my_id, y).is_none(),
//             "state was removed before insertion"
//         );
//     }
// }
