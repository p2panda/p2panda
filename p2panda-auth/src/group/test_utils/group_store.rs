// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::rc::Rc;

use thiserror::Error;

use crate::traits::{GroupStore, IdentityHandle};

use super::TestGroupState;

#[derive(Debug, Error)]
pub enum GroupStoreError {}

#[derive(Clone, Debug)]
pub struct TestGroupStore<ID>(Rc<RefCell<HashMap<ID, TestGroupState>>>)
where
    ID: IdentityHandle;

impl<ID> Default for TestGroupStore<ID>
where
    ID: IdentityHandle,
{
    fn default() -> Self {
        Self(Rc::new(RefCell::new(HashMap::new())))
    }
}

impl GroupStore<char> for TestGroupStore<char> {
    type Group = TestGroupState;

    type Error = GroupStoreError;

    fn get(&self, id: &char) -> Result<Option<Self::Group>, Self::Error> {
        let store = self.0.borrow();
        let group_y = store.get(id);
        Ok(group_y.cloned())
    }

    fn insert(&self, id: &char, group: &Self::Group) -> Result<(), Self::Error> {
        {
            let mut store = self.0.borrow_mut();
            store.insert(*id, group.clone());
        }
        Ok(())
    }
}
