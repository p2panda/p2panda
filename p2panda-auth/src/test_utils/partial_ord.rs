use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;
use std::rc::Rc;

use rand::RngCore;
use rand::rngs::StdRng;
use thiserror::Error;

use crate::group::{AuthState, GroupControlMessage, GroupCrdt, GroupCrdtError, GroupCrdtState};
use crate::test_utils::{Conditions, MemberId, MessageId, TestOperation, TestResolver};
use crate::traits::{Operation, Orderer};

pub type TestAuthState = AuthState<MemberId, MessageId, Conditions, TestOperation>;
pub type TestGroupState = GroupCrdtState<MemberId, MessageId, Conditions, TestOrderer>;
pub type TestGroup = GroupCrdt<MemberId, MessageId, Conditions, TestResolver, TestOrderer>;
pub type TestGroupError =
    GroupCrdtError<MemberId, MessageId, Conditions, TestResolver, TestOrderer>;

#[derive(Debug, Error)]
pub enum OrdererError {}

#[derive(Clone, Debug)]
pub struct TestOrdererState {
    pub my_id: MemberId,
    pub auth_heads: Rc<RefCell<Vec<MessageId>>>,
    pub orderer_y: PartialOrdererState<MessageId>,
    pub messages: HashMap<MessageId, TestOperation>,
    pub rng: StdRng,
}

impl TestOrdererState {
    pub fn my_id(&self) -> MemberId {
        self.my_id
    }
}

#[derive(Clone, Debug, Default)]
pub struct TestOrderer {}

impl TestOrderer {
    pub fn init(
        my_id: MemberId,
        auth_heads: Rc<RefCell<Vec<MessageId>>>,
        rng: StdRng,
    ) -> TestOrdererState {
        let y = TestOrdererState {
            my_id,
            auth_heads,
            messages: Default::default(),
            orderer_y: PartialOrdererState::default(),
            rng,
        };
        y
    }
}

impl Orderer<MemberId, MessageId, GroupControlMessage<MemberId, Conditions>> for TestOrderer {
    type State = TestOrdererState;

    type Error = OrdererError;

    type Operation = TestOperation;

    /// Construct the next operation which should include meta-data required for establishing order
    /// between different operations.
    ///
    /// In this implementation causal order is established between operations using a graph
    /// structure. Every operation contains a pointer to both the previous operations in a single auth
    /// group graph, and also the tips of any sub-group graphs.
    fn next_message(
        mut y: Self::State,
        control_message: &GroupControlMessage<MemberId, Conditions>,
    ) -> Result<(Self::State, Self::Operation), Self::Error> {
        // Get auth dependencies. These are the current heads of all groups.
        let auth_dependencies = y.auth_heads.borrow().to_owned();

        // Generate a new random operation id.
        let next_id = { y.rng.next_u32() };

        // Construct the actual operation.
        let operation = TestOperation {
            id: next_id,
            author: y.my_id(),
            dependencies: auth_dependencies.iter().cloned().collect::<Vec<_>>(),
            payload: control_message.clone(),
        };

        // Queue the operation in the orderer.
        //
        // Even though we know the operation is ready for processing (ordering dependencies are
        // met), we need to queue it so that the orderer progresses to the correct state.
        //
        // TODO: we should rather update the orderer state directly as this method (next_message) is
        // always called locally and we can assume that our own messages are processed immediately.
        let y_i = TestOrderer::queue(y, &operation)?;

        // Update the auth heads assuming that this message will be processed correctly.
        y_i.auth_heads.replace(vec![next_id]);

        Ok((y_i, operation))
    }

    fn queue(mut y: Self::State, message: &Self::Operation) -> Result<Self::State, Self::Error> {
        let id = message.id();

        {
            let dependencies = message.dependencies();

            if !PartialOrderer::ready(&y.orderer_y, &dependencies).unwrap() {
                let (orderer_y_i, _) =
                    PartialOrderer::mark_pending(y.orderer_y.clone(), id, dependencies.clone())
                        .unwrap();
                y.orderer_y = orderer_y_i;
            } else {
                let (orderer_y_i, _) = PartialOrderer::mark_ready(y.orderer_y.clone(), id).unwrap();
                let orderer_y_ii = PartialOrderer::process_pending(orderer_y_i, id).unwrap();
                y.orderer_y = orderer_y_ii;
            }
        }

        Ok(y)
    }

    fn next_ready_message(
        mut y: Self::State,
    ) -> Result<(Self::State, Option<Self::Operation>), Self::Error> {
        let next_msg = {
            let (orderer_y_i, msg) = PartialOrderer::take_next_ready(y.orderer_y.clone()).unwrap();
            y.orderer_y = orderer_y_i;
            msg
        };

        let next_msg = match next_msg {
            Some(msg) => y.messages.get(&msg).cloned(),
            None => None,
        };

        Ok((y, next_msg))
    }
}

/// Queue which checks if dependencies are met for an item and returning it as "ready".
///
/// Internally this assumes a structure where items can point at others as "dependencies", forming
/// an DAG (Directed Acyclic Graph). The "orderer" monitors incoming items, asserts if the
/// dependencies are met and yields a linearized sequence of "dependency checked" items.
#[derive(Debug)]
pub struct PartialOrderer<T> {
    _marker: PhantomData<T>,
}

#[derive(Clone, Debug)]
pub struct PartialOrdererState<T>
where
    T: PartialEq + Eq + StdHash,
{
    ready: HashSet<T>,
    ready_queue: VecDeque<T>,
    pending: HashMap<T, HashSet<(T, Vec<T>)>>,
}

impl<T> Default for PartialOrdererState<T>
where
    T: PartialEq + Eq + StdHash,
{
    fn default() -> Self {
        Self {
            ready: Default::default(),
            ready_queue: Default::default(),
            pending: Default::default(),
        }
    }
}

impl<T> PartialOrderer<T>
where
    T: Copy + Clone + PartialEq + Eq + StdHash,
{
    pub fn mark_ready(
        mut y: PartialOrdererState<T>,
        key: T,
    ) -> Result<(PartialOrdererState<T>, bool), PartialOrdererError> {
        let result = y.ready.insert(key);
        if result {
            y.ready_queue.push_back(key);
        }
        Ok((y, result))
    }

    pub fn mark_pending(
        mut y: PartialOrdererState<T>,
        key: T,
        dependencies: Vec<T>,
    ) -> Result<(PartialOrdererState<T>, bool), PartialOrdererError> {
        let insert_occured = false;
        for dep_key in &dependencies {
            if y.ready.contains(dep_key) {
                continue;
            }

            let dependents = y.pending.entry(*dep_key).or_default();
            dependents.insert((key, dependencies.clone()));
        }

        Ok((y, insert_occured))
    }

    #[allow(clippy::type_complexity)]
    pub fn get_next_pending(
        y: &PartialOrdererState<T>,
        key: T,
    ) -> Result<Option<HashSet<(T, Vec<T>)>>, PartialOrdererError> {
        Ok(y.pending.get(&key).cloned())
    }

    pub fn take_next_ready(
        mut y: PartialOrdererState<T>,
    ) -> Result<(PartialOrdererState<T>, Option<T>), PartialOrdererError> {
        let result = y.ready_queue.pop_front();
        Ok((y, result))
    }

    pub fn remove_pending(
        mut y: PartialOrdererState<T>,
        key: T,
    ) -> Result<(PartialOrdererState<T>, bool), PartialOrdererError> {
        let result = y.pending.remove(&key).is_some();
        Ok((y, result))
    }

    pub fn ready(
        y: &PartialOrdererState<T>,
        dependencies: &[T],
    ) -> Result<bool, PartialOrdererError> {
        let deps_set = HashSet::from_iter(dependencies.iter().cloned());
        let result = y.ready.is_superset(&deps_set);
        Ok(result)
    }

    pub fn process_pending(
        y: PartialOrdererState<T>,
        key: T,
    ) -> Result<PartialOrdererState<T>, PartialOrdererError> {
        // Get all items which depend on the passed key.
        let Some(dependents) = Self::get_next_pending(&y, key)? else {
            return Ok(y);
        };

        // For each dependent check if it has all it's dependencies met, if not then we do nothing
        // as it is still in a pending state.
        let mut y_loop = y;
        for (next_key, next_deps) in dependents {
            if !Self::ready(&y_loop, &next_deps)? {
                continue;
            }

            let (y_next, _) = Self::mark_ready(y_loop, next_key)?;
            y_loop = y_next;

            // Recurse down the dependency graph by now checking any pending items which depend on
            // the current item.
            let y_next = Self::process_pending(y_loop, next_key)?;
            y_loop = y_next;
        }

        // Finally remove this item from the pending items queue.
        let (y_i, _) = Self::remove_pending(y_loop, key)?;

        Ok(y_i)
    }
}

#[derive(Debug, Error)]
pub enum PartialOrdererError {
    // TODO: For now the orderer API is infallible, but we keep the error type around for later, as
    // in it's current form the orderer would need to keep too much memory around for processing
    // and we'll probably start to introduce a persistence backend (which can fail).
}
