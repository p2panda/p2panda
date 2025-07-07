// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::rc::Rc;

use rand::RngCore;
use rand::rngs::StdRng;
use thiserror::Error;

use crate::group::{GroupAction, GroupControlMessage, GroupMember};
use crate::test_utils::{
    Conditions, MemberId, MessageId, PartialOrderer, PartialOrdererState, TestGroupState,
    TestGroupStore,
};
use crate::traits::{GroupStore, Operation, Orderer};

#[derive(Debug, Error)]
pub enum OrdererError {}

#[derive(Clone, Debug)]
pub struct TestOrdererState {
    pub inner: Rc<RefCell<TestOrdererStateInner>>,
}

#[derive(Clone, Debug)]
pub struct TestOrdererStateInner {
    pub my_id: MemberId,
    pub group_store: TestGroupStore,
    pub orderer_y: PartialOrdererState<MessageId>,
    pub messages: HashMap<MessageId, TestOperation>,
    pub rng: StdRng,
}

impl TestOrdererState {
    pub fn new(my_id: MemberId, group_store: TestGroupStore, rng: StdRng) -> Self {
        let inner = TestOrdererStateInner {
            my_id,
            group_store,
            messages: Default::default(),
            orderer_y: PartialOrdererState::default(),
            rng,
        };
        Self {
            inner: Rc::new(RefCell::new(inner)),
        }
    }

    pub fn my_id(&self) -> MemberId {
        self.inner.borrow().my_id
    }
}

#[derive(Clone, Debug, Default)]
pub struct TestOrderer {}

impl Orderer<MemberId, MessageId, GroupControlMessage<MemberId, Conditions>> for TestOrderer {
    type State = TestOrdererState;

    type Error = OrdererError;

    type Operation = TestOperation;

    /// Construct the next operation which should include meta-data required for establishing order
    /// between different operations.
    ///
    /// In this implementation causal order is established between operations using a graph
    /// structure. Every operation contains a pointer to both the previous operations in a single auth
    /// group graph, and also the tips of any sub-group graphs.
    fn next_message(
        y: Self::State,
        control_message: &GroupControlMessage<MemberId, Conditions>,
    ) -> Result<(Self::State, Self::Operation), Self::Error> {
        let group_id = control_message.group_id();
        let group_y = {
            let y_inner = y.inner.borrow();

            // Instantiate a new group.
            let mut group_y = TestGroupState::new(
                y_inner.my_id,
                group_id,
                y_inner.group_store.clone(),
                y.clone(),
            );

            // If this isn't a create message, retrieve the current group state from the store.
            if !control_message.is_create() {
                let y = TestGroupStore::get(&y_inner.group_store, &group_id)
                    .expect("get group state from store")
                    .expect("group exists");
                group_y = y;
            }

            group_y
        };

        // Get the "dependencies" of this operation. Dependencies are any other operations from any
        // group which should be processed before this one. The transitive_heads method traverses
        // the current group graph to all tips, and recurses into any sub-groups.
        let mut dependencies = group_y
            .transitive_heads()
            .expect("retrieve transitive heads");

        // If this operation adds a new member to the group, and that member itself is a
        // sub-group, include their current transitive heads in our dependencies.
        if let GroupControlMessage {
            action:
                GroupAction::Add {
                    member: GroupMember::Group(id),
                    ..
                },
            ..
        } = control_message
        {
            let added_sub_group = group_y.get_sub_group(*id).expect("sub-group exists");
            dependencies.extend(
                &added_sub_group
                    .transitive_heads()
                    .expect("retrieve transitive heads"),
            );
        };

        // If this is a create action, check if any of the initial members are sub-groups and if
        // so include their current transitive heads in our dependencies.
        if let GroupControlMessage {
            action: GroupAction::Create { initial_members },
            ..
        } = control_message
        {
            for (member, _) in initial_members {
                if let GroupMember::Group(id) = member {
                    let sub_group = group_y.get_sub_group(*id).expect("sub-group exists");
                    dependencies.extend(
                        &sub_group
                            .transitive_heads()
                            .expect("retrieve transitive heads"),
                    );
                }
            }
        };

        // The previous field includes only the tip operations for the target group graph.
        let previous = group_y.heads();

        // Generate a new random operation id.
        let next_id = {
            let mut y_mut = y.inner.borrow_mut();
            y_mut.rng.next_u32()
        };

        // Construct the actual operation.
        let operation = TestOperation {
            id: next_id,
            author: y.my_id(),
            dependencies: dependencies.into_iter().collect::<Vec<_>>(),
            previous: previous.into_iter().collect::<Vec<_>>(),
            payload: control_message.clone(),
        };

        // Queue the operation in the orderer.
        //
        // Even though we know the operation is ready for processing (ordering dependencies are
        // met), we need to queue it so that the orderer progresses to the correct state.
        //
        // TODO: we should rather update the orderer state directly as this method (next_message) is
        // always called locally and we can assume that our own messages are processed immediately.
        let y_i = TestOrderer::queue(y, &operation)?;

        Ok((y_i, operation))
    }

    fn queue(y: Self::State, message: &Self::Operation) -> Result<Self::State, Self::Error> {
        let id = message.id();

        {
            let mut inner: std::cell::RefMut<'_, TestOrdererStateInner> = y.inner.borrow_mut();
            inner.messages.insert(id, message.clone());

            let dependencies = message.dependencies();

            if !PartialOrderer::ready(&inner.orderer_y, &dependencies).unwrap() {
                let (orderer_y_i, _) =
                    PartialOrderer::mark_pending(inner.orderer_y.clone(), id, dependencies.clone())
                        .unwrap();
                inner.orderer_y = orderer_y_i;
            } else {
                let (orderer_y_i, _) =
                    PartialOrderer::mark_ready(inner.orderer_y.clone(), id).unwrap();
                let orderer_y_ii = PartialOrderer::process_pending(orderer_y_i, id).unwrap();
                inner.orderer_y = orderer_y_ii;
            }
        }

        Ok(y)
    }

    fn next_ready_message(
        y: Self::State,
    ) -> Result<(Self::State, Option<Self::Operation>), Self::Error> {
        let next_msg = {
            let mut inner = y.inner.borrow_mut();
            let (orderer_y_i, msg) =
                PartialOrderer::take_next_ready(inner.orderer_y.clone()).unwrap();

            inner.orderer_y = orderer_y_i;
            msg
        };

        let next_msg = match next_msg {
            Some(msg) => y.inner.borrow().messages.get(&msg).cloned(),
            None => None,
        };

        Ok((y, next_msg))
    }
}

#[derive(Clone, Debug)]
pub struct TestOperation {
    pub id: u32,
    pub author: char,
    pub dependencies: Vec<u32>,
    pub previous: Vec<u32>,
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

    fn previous(&self) -> Vec<u32> {
        self.previous.clone()
    }

    fn payload(&self) -> GroupControlMessage<char, ()> {
        self.payload.clone()
    }
}
