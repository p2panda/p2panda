use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::{
    group::GroupStateInner,
    traits::{GroupStore, IdentityHandle, OperationId},
};

#[derive(Clone, Debug)]
pub struct MemoryStore<ID, OP, MSG>(Rc<RefCell<HashMap<ID, GroupStateInner<ID, OP, MSG>>>>)
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    MSG: Clone;

impl<ID, OP, MSG> Default for MemoryStore<ID, OP, MSG>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    MSG: Clone,
{
    fn default() -> Self {
        Self(Rc::new(RefCell::new(HashMap::new())))
    }
}

impl<ID, OP, MSG> GroupStore<ID, OP, MSG> for MemoryStore<ID, OP, MSG>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    MSG: Clone,
{
    fn get(&self, id: &ID) -> Option<GroupStateInner<ID, OP, MSG>> {
        let store = self.0.borrow();
        let inner = store.get(id);
        inner.cloned()
    }

    fn insert(
        &self,
        id: &ID,
        group: GroupStateInner<ID, OP, MSG>,
    ) -> Option<GroupStateInner<ID, OP, MSG>> {
        let mut store = self.0.borrow_mut();
        store.insert(*id, group)
    }

    fn remove(&self, id: &ID) -> Option<GroupStateInner<ID, OP, MSG>> {
        let mut store = self.0.borrow_mut();
        store.remove(id)
    }
}
