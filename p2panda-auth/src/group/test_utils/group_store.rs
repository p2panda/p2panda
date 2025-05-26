// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::fmt::Debug;
use std::rc::Rc;
use std::{cell::RefCell, marker::PhantomData};

use thiserror::Error;

use crate::traits::{GroupStore, IdentityHandle};

use super::TestGroupState;

#[derive(Debug, Clone)]
pub struct TestGroupStoreState<ID>(Rc<RefCell<HashMap<ID, TestGroupState>>>)
where
    ID: IdentityHandle;

#[derive(Debug, Error)]
pub enum GroupStoreError {}

#[derive(Clone, Debug)]
pub struct TestGroupStore<ID> {
    phantom: PhantomData<ID>,
}

impl<ID> Default for TestGroupStoreState<ID>
where
    ID: IdentityHandle,
{
    fn default() -> Self {
        Self(Rc::new(RefCell::new(HashMap::new())))
    }
}

impl GroupStore<char> for TestGroupStore<char> {
    type State = TestGroupStoreState<char>;
    type Group = TestGroupState;

    type Error = GroupStoreError;

    fn get(y: &Self::State, id: &char) -> Result<Option<Self::Group>, Self::Error> {
        let store = y.0.borrow();
        let group_y = store.get(id);
        Ok(group_y.cloned())
    }

    fn insert(y: Self::State, id: &char, group: &Self::Group) -> Result<Self::State, Self::Error> {
        {
            let mut store = y.0.borrow_mut();
            store.insert(*id, group.clone());
        }
        Ok(y)
    }
}
