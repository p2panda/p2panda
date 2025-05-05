use std::collections::VecDeque;
use std::fmt::Display;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::group::GroupControlMessage;
use crate::traits::{IdentityHandle, Operation, Ordering};

pub type TestOperationID = u32;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestOrderer {}

#[derive(Debug, Error)]
pub enum OrdererError {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestOrdererState<ID> {
    pub my_id: ID,
    pub operations: VecDeque<TestOperation<ID, TestOperationID>>,
}

impl<ID> Ordering<ID, TestOperationID, GroupControlMessage<ID, TestOperationID>> for TestOrderer
where
    ID: IdentityHandle + Display + Serialize + for<'a> Deserialize<'a>,
{
    type State = TestOrdererState<ID>;

    type Message = TestOperation<ID, TestOperationID>;

    type Error = OrdererError;

    fn next_message(
        y: Self::State,
        dependencies: Vec<TestOperationID>,
        payload: &GroupControlMessage<ID, TestOperationID>,
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        let next_operation = TestOperation {
            id: rand::random(),
            sender: y.my_id,
            dependencies,
            payload: payload.clone(),
        };
        Ok((y, next_operation))
    }

    fn queue(mut y: Self::State, operation: &Self::Message) -> Result<Self::State, Self::Error> {
        y.operations.push_back(operation.clone());
        Ok(y)
    }

    fn next_ready_message(
        mut y: Self::State,
    ) -> Result<(Self::State, Option<Self::Message>), Self::Error> {
        let operation = y.operations.pop_front();
        Ok((y, operation))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestOperation<ID, OP> {
    pub id: OP,
    pub sender: ID,
    pub dependencies: Vec<OP>,
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

    fn payload(&self) -> &GroupControlMessage<ID, OP> {
        &self.payload
    }
}
