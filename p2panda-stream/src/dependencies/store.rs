use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash as StdHash;

use super::DependencyCheckerError;

/// Trait defining a store API for handling ready and pending dependencies.
///
/// An implementation of this store trait provides the following functionality:
/// - maintain a list of all items which have all their dependencies met
/// - maintain a list of items which don't have their dependencies met
/// - return all pending items which depend on a given item key
pub trait DependencyStore<K>
where
    K: Clone + Copy + StdHash + PartialEq + Eq,
{
    /// Add an item to the store which has all it's dependencies met already.
    async fn add_ready(&mut self, key: K) -> Result<bool, DependencyCheckerError>;

    /// Add an item which does not have all it's dependencies met yet.
    async fn add_pending(
        &mut self,
        key: K,
        dependencies: Vec<K>,
    ) -> Result<bool, DependencyCheckerError>;

    /// Get all pending items which directly depend on the given key.
    async fn get_next_pending(
        &self,
        key: K,
    ) -> Result<Option<HashSet<(K, Vec<K>)>>, DependencyCheckerError>;

    /// Remove all items from the pending queue which depend on the passed key.
    async fn remove_pending(&mut self, key: K) -> Result<bool, DependencyCheckerError>;

    /// Returns `true` of all the passed keys are present in the ready list.
    async fn ready(&self, keys: &[K]) -> Result<bool, DependencyCheckerError>;
}

/// Memory implementation of the `DependencyStore` trait.
#[derive(Clone)]
pub struct MemoryStore<K> {
    pub(crate) ready: HashSet<K>,
    pub(crate) pending: HashMap<K, HashSet<(K, Vec<K>)>>,
}

impl<K> Default for MemoryStore<K> {
    fn default() -> Self {
        Self {
            ready: HashSet::new(),
            pending: HashMap::new(),
        }
    }
}

impl<K> DependencyStore<K> for MemoryStore<K>
where
    K: Clone + Copy + Debug + StdHash + PartialEq + Eq,
{
    async fn add_ready(&mut self, key: K) -> Result<bool, DependencyCheckerError> {
        let result = self.ready.insert(key);
        Ok(result)
    }

    async fn add_pending(
        &mut self,
        key: K,
        dependencies: Vec<K>,
    ) -> Result<bool, DependencyCheckerError> {
        let insert_occured = false;
        for dep_key in &dependencies {
            if self.ready.contains(dep_key) {
                continue;
            }

            let dependents = self.pending.entry(*dep_key).or_default();
            dependents.insert((key, dependencies.clone()));
        }

        Ok(insert_occured)
    }

    async fn get_next_pending(
        &self,
        key: K,
    ) -> Result<Option<HashSet<(K, Vec<K>)>>, DependencyCheckerError> {
        Ok(self.pending.get(&key).cloned())
    }

    async fn remove_pending(&mut self, key: K) -> Result<bool, DependencyCheckerError> {
        Ok(self.pending.remove(&key).is_some())
    }

    async fn ready(&self, dependencies: &[K]) -> Result<bool, DependencyCheckerError> {
        let deps_set = HashSet::from_iter(dependencies.iter().cloned());
        let result = self.ready.is_superset(&deps_set);
        Ok(result)
    }
}
