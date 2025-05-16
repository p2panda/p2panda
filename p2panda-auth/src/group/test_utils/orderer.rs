use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::rc::Rc;

use rand::RngCore;
use rand::rngs::StdRng;
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
    pub orderer_y: PartialOrdererState<MessageId>,
    pub messages: HashMap<MessageId, TestOperation<MemberId, MessageId>>,
    pub rng: StdRng,
}

impl TestOrdererState {
    pub fn new(
        my_id: MemberId,
        group_store_y: TestGroupStoreState<GroupId, TestGroupStateInner>,
        rng: StdRng,
    ) -> Self {
        let inner = TestOrdererStateInner {
            my_id,
            group_store_y,
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
        let next_id = {
            let mut y_mut = y.inner.borrow_mut();
            y_mut.rng.next_u32()
        };

        let message = TestOperation {
            id: next_id,
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
    ) -> Result<(Self::State, Option<Self::Message>), Self::Error> {
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
