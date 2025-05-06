use crate::group::GroupStateInner;

use super::{IdentityHandle, OperationId};

pub trait GroupStore<ID, OP, MSG>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    MSG: Clone,
{
    fn get(&self, id: &ID) -> Option<GroupStateInner<ID, OP, MSG>>;
    fn insert(
        &self,
        id: &ID,
        group: GroupStateInner<ID, OP, MSG>,
    ) -> Option<GroupStateInner<ID, OP, MSG>>;
    fn remove(&self, id: &ID) -> Option<GroupStateInner<ID, OP, MSG>>;
}
