// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::convert::Infallible;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use crate::traits::{GroupMembership, IdentityHandle, OperationId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestDgm<ID, OP> {
    _marker: PhantomData<(ID, OP)>,
}

impl<ID, OP> TestDgm<ID, OP>
where
    ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
{
    pub fn init(my_id: ID) -> TestDgmState<ID, OP> {
        TestDgmState {
            my_id,
            members: HashSet::new(),
            _marker: PhantomData,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestDgmState<ID, OP>
where
    ID: IdentityHandle,
{
    my_id: ID,
    members: HashSet<ID>,
    _marker: PhantomData<OP>,
}

impl<ID, OP> GroupMembership<ID, OP> for TestDgm<ID, OP>
where
    ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
    OP: OperationId + Serialize + for<'a> Deserialize<'a>,
{
    type State = TestDgmState<ID, OP>;

    type Error = Infallible;

    fn create(my_id: ID, initial_members: &[ID]) -> Result<Self::State, Self::Error> {
        Ok(TestDgmState {
            my_id,
            members: HashSet::from_iter(initial_members.iter().cloned()),
            _marker: PhantomData,
        })
    }

    fn from_welcome(my_id: ID, y: Self::State) -> Result<Self::State, Self::Error> {
        Ok(TestDgmState {
            my_id,
            members: y.members,
            _marker: PhantomData,
        })
    }

    fn add(
        mut y: Self::State,
        _adder: ID,
        added: ID,
        _operation_id: OP,
    ) -> Result<Self::State, Self::Error> {
        y.members.insert(added);
        Ok(y)
    }

    fn remove(
        mut y: Self::State,
        _remover: ID,
        removed: &ID,
        _operation_id: OP,
    ) -> Result<Self::State, Self::Error> {
        y.members.remove(removed);
        Ok(y)
    }

    fn members(y: &Self::State) -> Result<HashSet<ID>, Self::Error> {
        Ok(y.members.clone())
    }
}
