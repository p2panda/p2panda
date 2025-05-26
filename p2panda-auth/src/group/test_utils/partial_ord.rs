// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Queue which checks if dependencies are met for an item and returning it as "ready".
///
/// Internally this assumes a structure where items can point at others as "dependencies", forming
/// an DAG (Directed Acyclic Graph). The "orderer" monitors incoming items, asserts if the
/// dependencies are met and yields a linearized sequence of "dependency checked" items.
#[derive(Debug)]
pub struct PartialOrderer<T> {
    _marker: PhantomData<T>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
