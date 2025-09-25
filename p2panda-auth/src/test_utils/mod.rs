// SPDX-License-Identifier: MIT OR Apache-2.0

//! Test utilities.

pub mod no_ord;
pub mod partial_ord;

use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::Access;
use crate::group::resolver::StrongRemove;
use crate::group::{GroupAction, GroupControlMessage, GroupCrdt, GroupCrdtState, GroupMember};
use crate::traits::{IdentityHandle, Operation, OperationId, Orderer};

impl IdentityHandle for char {}
impl OperationId for u32 {}

pub type MemberId = char;
pub type MessageId = u32;
pub type Conditions = ();
pub type TestResolver = StrongRemove<MemberId, MessageId, Conditions, TestOperation>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TestOperation {
    pub id: u32,
    pub author: char,
    pub dependencies: Vec<u32>,
    pub payload: GroupControlMessage<char, ()>,
}

impl Operation<char, u32, GroupControlMessage<char, ()>> for TestOperation {
    fn id(&self) -> u32 {
        self.id
    }

    fn author(&self) -> char {
        self.author
    }

    fn dependencies(&self) -> Vec<u32> {
        self.dependencies.clone()
    }

    fn payload(&self) -> GroupControlMessage<char, ()> {
        self.payload.clone()
    }
}

fn make_group_op(
    actor_id: MemberId,
    operation_id: MessageId,
    group_id: MemberId,
    action: GroupAction<MemberId, ()>,
    dependencies: Vec<MessageId>,
) -> TestOperation {
    let control_message = GroupControlMessage { group_id, action };
    TestOperation {
        id: operation_id,
        author: actor_id,
        dependencies,
        payload: control_message,
    }
}

pub fn create_group(
    actor_id: MemberId,
    operation_id: MessageId,
    group_id: MemberId,
    initial_members: Vec<(GroupMember<MemberId>, Access<()>)>,
    dependencies: Vec<MessageId>,
) -> TestOperation {
    make_group_op(
        actor_id,
        operation_id,
        group_id,
        GroupAction::Create { initial_members },
        dependencies,
    )
}

pub fn add_member(
    actor_id: MemberId,
    operation_id: MessageId,
    group_id: MemberId,
    member: GroupMember<MemberId>,
    access: Access<()>,
    dependencies: Vec<MessageId>,
) -> TestOperation {
    make_group_op(
        actor_id,
        operation_id,
        group_id,
        GroupAction::Add { member, access },
        dependencies,
    )
}

pub fn remove_member(
    actor_id: MemberId,
    operation_id: MessageId,
    group_id: MemberId,
    member: GroupMember<MemberId>,
    dependencies: Vec<MessageId>,
) -> TestOperation {
    make_group_op(
        actor_id,
        operation_id,
        group_id,
        GroupAction::Remove { member },
        dependencies,
    )
}

pub fn promote_member(
    actor_id: MemberId,
    operation_id: MessageId,
    group_id: MemberId,
    member: GroupMember<MemberId>,
    access: Access<()>,
    dependencies: Vec<MessageId>,
) -> TestOperation {
    make_group_op(
        actor_id,
        operation_id,
        group_id,
        GroupAction::Promote { member, access },
        dependencies,
    )
}

pub fn demote_member(
    actor_id: MemberId,
    operation_id: MessageId,
    group_id: MemberId,
    member: GroupMember<MemberId>,
    access: Access<()>,
    dependencies: Vec<MessageId>,
) -> TestOperation {
    make_group_op(
        actor_id,
        operation_id,
        group_id,
        GroupAction::Demote { member, access },
        dependencies,
    )
}

pub fn sync<ORD>(
    y: GroupCrdtState<MemberId, MessageId, Conditions, ORD>,
    ops: &[TestOperation],
) -> GroupCrdtState<MemberId, MessageId, Conditions, ORD>
where
    ORD: Orderer<
            MemberId,
            MessageId,
            GroupControlMessage<MemberId, Conditions>,
            Operation = TestOperation,
        > + Debug,
    ORD::Operation: Clone,
{
    ops.iter().fold(y, |g, op| {
        GroupCrdt::<MemberId, MessageId, Conditions, TestResolver, ORD>::process(g, op).unwrap()
    })
}

pub fn assert_members<ORD>(
    y: &GroupCrdtState<MemberId, MessageId, Conditions, ORD>,
    group_id: MemberId,
    expected: &[(char, Access<()>)],
) where
    ORD: Orderer<MemberId, MessageId, GroupControlMessage<MemberId, Conditions>> + Debug,
    ORD::Operation: Clone,
{
    let mut actual = y.members(group_id);
    let mut expected = expected.to_vec();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}
