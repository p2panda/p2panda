use std::collections::HashMap;
use std::fmt::Debug;
use std::rc::Rc;
use std::{cell::RefCell, marker::PhantomData};

use thiserror::Error;

use crate::traits::{GroupStore, IdentityHandle};

use super::TestGroupStateInner;

#[derive(Debug, Clone)]
pub struct TestGroupStoreState<ID, G>(Rc<RefCell<HashMap<ID, G>>>)
where
    ID: IdentityHandle;

#[derive(Debug, Error)]
pub enum GroupStoreError {}

#[derive(Clone, Debug)]
pub struct TestGroupStore<ID, G> {
    phantom: PhantomData<(ID, G)>,
}

impl<ID> Default for TestGroupStoreState<ID, TestGroupStateInner>
where
    ID: IdentityHandle,
{
    fn default() -> Self {
        Self(Rc::new(RefCell::new(HashMap::new())))
    }
}

impl GroupStore<char, TestGroupStateInner> for TestGroupStore<char, TestGroupStateInner> {
    type State = TestGroupStoreState<char, TestGroupStateInner>;

    type Error = GroupStoreError;

    fn get(y: &Self::State, id: &char) -> Result<Option<TestGroupStateInner>, Self::Error> {
        let store = y.0.borrow();
        let inner = store.get(id);
        Ok(inner.cloned())
    }

    fn insert(
        y: Self::State,
        id: &char,
        group: &TestGroupStateInner,
    ) -> Result<Self::State, Self::Error> {
        {
            let mut store = y.0.borrow_mut();
            store.insert(*id, group.clone());
        }
        Ok(y)
    }
}
