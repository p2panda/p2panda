// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::rc::Rc;

use thiserror::Error;

use crate::test_utils::{
    Conditions, MemberId, MessageId, TestGroupState, TestOrderer, TestResolver,
};
use crate::traits::GroupStore;

#[derive(Debug, Error)]
pub enum GroupStoreError {}

#[derive(Clone, Debug)]
pub struct TestGroupStore(Rc<RefCell<HashMap<MemberId, TestGroupState>>>);

impl Default for TestGroupStore {
    fn default() -> Self {
        Self(Rc::new(RefCell::new(HashMap::new())))
    }
}

impl GroupStore<MemberId, MessageId, Conditions, TestResolver, TestOrderer> for TestGroupStore {
    type Error = GroupStoreError;

    fn get(&self, id: &char) -> Result<Option<TestGroupState>, Self::Error> {
        let store = self.0.borrow();
        let group_y = store.get(id);
        Ok(group_y.cloned())
    }

    fn insert(&self, id: &char, group: &TestGroupState) -> Result<(), Self::Error> {
        {
            let mut store = self.0.borrow_mut();
            store.insert(*id, group.clone());
        }
        Ok(())
    }
}
