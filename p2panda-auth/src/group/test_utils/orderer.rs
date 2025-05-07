use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::rc::Rc;

// use p2panda_stream::partial::{MemoryStore, PartialOrder};
use thiserror::Error;

use crate::group::GroupControlMessage;
use crate::traits::{Operation, Ordering};

use super::{
    GroupId, MemberId, MessageId, PartialOrderer, PartialOrdererState, TestGroupStateInner,
    TestGroupStoreState,
};

#[derive(Debug, Error)]
pub enum OrdererError {}

#[derive(Clone, Debug)]
pub struct TestOrdererState {
    pub inner: Rc<RefCell<TestOrdererStateInner>>,
}

#[derive(Clone, Debug)]
pub struct TestOrdererStateInner {
    pub my_id: MemberId,
    pub group_store_y: TestGroupStoreState<GroupId, TestGroupStateInner>,
    pub partial_orderer_y: PartialOrdererState<MessageId>,
    pub messages: HashMap<MessageId, TestOperation<MemberId, MessageId>>,
}

impl TestOrdererState {
    pub fn new(
        my_id: MemberId,
        group_store_y: TestGroupStoreState<GroupId, TestGroupStateInner>,
    ) -> Self {
        let inner = TestOrdererStateInner {
            my_id,
            group_store_y,
            messages: Default::default(),
            partial_orderer_y: PartialOrdererState::default(),
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

impl Ordering<MemberId, MessageId, GroupControlMessage<MemberId, MessageId>> for TestOrderer {
    type State = TestOrdererState;

    type Error = OrdererError;

    type Message = TestOperation<MemberId, MessageId>;

    fn next_message(
        y: Self::State,
        dependencies: Vec<MessageId>,
        previous: Vec<MessageId>,
        payload: &GroupControlMessage<MemberId, MessageId>,
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        let message = TestOperation {
            id: rand::random(),
            sender: y.my_id(),
            dependencies,
            previous,
            payload: payload.clone(),
        };

        // Queue locally created messages.
        //
        // @TODO: not sure we actually want to do this here, maybe it should be taken care of
        // outside this method?
        let y_i = Self::queue(y, &message)?;

        Ok((y_i, message))
    }

    fn queue(y: Self::State, message: &Self::Message) -> Result<Self::State, Self::Error> {
        let id = message.id();

        {
            let mut inner = y.inner.borrow_mut();
            inner.partial_orderer_y =
                PartialOrderer::process_pending(inner.partial_orderer_y.clone(), id).unwrap();
        }

        Ok(y)
    }

    fn next_ready_message(
        y: Self::State,
    ) -> Result<(Self::State, Option<Self::Message>), Self::Error> {
        let mut next_msg = None;
        {
            let mut inner = y.inner.borrow_mut();
            let (partial_orderer_y_i, msg) =
                PartialOrderer::take_next_ready(inner.partial_orderer_y.clone()).unwrap();
            inner.partial_orderer_y = partial_orderer_y_i;
            next_msg = msg;
        }

        let next_msg = match next_msg {
            Some(msg) => y.inner.borrow().messages.get(&msg).cloned(),
            None => None,
        };

        Ok((y, next_msg))
    }
}

#[derive(Clone, Debug)]
pub struct TestOperation<ID, OP> {
    pub id: OP,
    pub sender: ID,
    pub dependencies: Vec<OP>,
    pub previous: Vec<OP>,
    pub payload: GroupControlMessage<ID, OP>,
}

impl<ID, OP> Operation<ID, OP, GroupControlMessage<ID, OP>> for TestOperation<ID, OP>
where
    ID: Copy,
    OP: Copy,
{
    fn id(&self) -> OP {
        self.id
    }

    fn sender(&self) -> ID {
        self.sender
    }

    fn dependencies(&self) -> &Vec<OP> {
        &self.dependencies
    }

    fn previous(&self) -> &Vec<OP> {
        &self.previous
    }

    fn payload(&self) -> &GroupControlMessage<ID, OP> {
        &self.payload
    }
}
