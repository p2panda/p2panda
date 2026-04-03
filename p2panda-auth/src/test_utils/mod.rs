// SPDX-License-Identifier: MIT OR Apache-2.0

//! Test utilities.

use std::fmt::Debug;

use p2panda_stream::orderer::Ordering;
use serde::{Deserialize, Serialize};

use crate::Access;
use crate::group::resolver::StrongRemove;
use crate::group::{GroupAction, GroupCrdt, GroupCrdtError, GroupCrdtState, GroupMember};
use crate::traits::{IdentityHandle, Operation, OperationId};

impl IdentityHandle for char {}
impl OperationId for u32 {}

pub type MemberId = char;
pub type MessageId = u32;
pub type Conditions = ();
pub type TestGroupState = GroupCrdtState<MemberId, MessageId, TestOperation, Conditions>;
pub type TestGroup = GroupCrdt<MemberId, MessageId, TestOperation, Conditions, TestResolver>;
pub type TestResolver = StrongRemove<MemberId, MessageId, TestOperation, Conditions>;
pub type TestGroupError =
    GroupCrdtError<MemberId, MessageId, TestOperation, Conditions, TestResolver>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TestOperation {
    pub id: u32,
    pub author: char,
    pub dependencies: Vec<u32>,
    pub group_id: char,
    pub action: GroupAction<char, Conditions>,
}

impl Operation<char, u32, Conditions> for TestOperation {
    fn id(&self) -> u32 {
        self.id
    }

    fn author(&self) -> char {
        self.author
    }

    fn dependencies(&self) -> Vec<u32> {
        self.dependencies.clone()
    }

    fn group_id(&self) -> char {
        self.group_id
    }

    fn action(&self) -> GroupAction<char, Conditions> {
        self.action.clone()
    }
}

impl Ordering<u32> for TestOperation {
    fn dependencies(&self) -> &[u32] {
        &self.dependencies
    }
}

fn make_group_op(
    actor_id: MemberId,
    operation_id: MessageId,
    group_id: MemberId,
    action: GroupAction<MemberId, ()>,
    dependencies: Vec<MessageId>,
) -> TestOperation {
    TestOperation {
        id: operation_id,
        author: actor_id,
        dependencies,
        group_id,
        action,
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

pub fn sync(
    y: GroupCrdtState<MemberId, MessageId, TestOperation, Conditions>,
    ops: &[TestOperation],
) -> GroupCrdtState<MemberId, MessageId, TestOperation, Conditions> {
    ops.iter().fold(y, |g, op| {
        GroupCrdt::<MemberId, MessageId, TestOperation, Conditions, TestResolver>::process(g, op)
            .unwrap()
    })
}

pub fn assert_members(
    y: &GroupCrdtState<MemberId, MessageId, TestOperation, Conditions>,
    group_id: MemberId,
    expected: &[(char, Access<()>)],
) {
    let mut actual = y.members(group_id);
    let mut expected = expected.to_vec();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}

pub fn setup_logging() {
    if std::env::var("RUST_LOG").is_ok() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();
    }
}
