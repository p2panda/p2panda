// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::convert::Infallible;
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rand::Rng;
use rand::seq::IteratorRandom;

use crate::address_book::{AddressBookStore, NodeInfo};

#[derive(Clone, Debug)]
pub struct AddressBookMemoryStore<R, ID, N> {
    rng: Rc<RefCell<R>>,
    node_infos: Rc<RefCell<BTreeMap<ID, N>>>,
    node_infos_last_changed: Rc<RefCell<BTreeMap<ID, u64>>>,
    topics: Rc<RefCell<BTreeMap<ID, HashSet<[u8; 32]>>>>,
}

impl<R, ID, N> AddressBookMemoryStore<R, ID, N> {
    pub fn new(rng: R) -> Self {
        Self {
            rng: Rc::new(RefCell::new(rng)),
            node_infos: Rc::new(RefCell::new(BTreeMap::new())),
            node_infos_last_changed: Rc::new(RefCell::new(BTreeMap::new())),
            topics: Rc::new(RefCell::new(BTreeMap::new())),
        }
    }

    /// Updates the "last changed" timestamp for a node.
    ///
    /// Remember at what time this entry got changed, this helps us later to do garbage
    /// collection of "old" entries.
    async fn update_last_changed(&self, id: ID)
    where
        ID: Ord,
    {
        let mut node_infos_last_changed = self.node_infos_last_changed.borrow_mut();
        node_infos_last_changed.insert(id, current_timestamp());
    }

    #[cfg(test)]
    pub async fn set_last_changed(&self, id: ID, timestamp: u64)
    where
        ID: Ord,
    {
        let mut node_infos_last_changed = self.node_infos_last_changed.borrow_mut();
        node_infos_last_changed.insert(id, timestamp);
    }
}

impl<R, ID, N> AddressBookStore<ID, N> for AddressBookMemoryStore<R, ID, N>
where
    R: Rng,
    ID: Clone + Ord,
    N: Clone + NodeInfo<ID>,
{
    type Error = Infallible;

    async fn insert_node_info(&self, info: N) -> Result<bool, Self::Error> {
        self.update_last_changed(info.id()).await;
        let mut node_infos = self.node_infos.borrow_mut();
        Ok(node_infos.insert(info.id(), info).is_none())
    }

    async fn node_info(&self, id: &ID) -> Result<Option<N>, Self::Error> {
        let node_infos = self.node_infos.borrow();
        Ok(node_infos.get(id).cloned())
    }

    async fn node_topics(&self, id: &ID) -> Result<HashSet<[u8; 32]>, Self::Error> {
        let topics = self.topics.borrow();
        let result = topics.get(id).cloned().unwrap_or(HashSet::new());
        Ok(result)
    }

    async fn selected_node_infos(&self, ids: &[ID]) -> Result<Vec<N>, Self::Error> {
        let node_infos = self.node_infos.borrow();
        let result = node_infos
            .iter()
            .filter_map(
                |(id, info)| {
                    if ids.contains(id) { Some(info) } else { None }
                },
            )
            .cloned()
            .collect();
        Ok(result)
    }

    async fn all_node_infos(&self) -> Result<Vec<N>, Self::Error> {
        let node_infos = self.node_infos.borrow();
        Ok(node_infos
            .values()
            .filter(|info| !info.is_stale())
            .cloned()
            .collect())
    }

    async fn all_nodes_len(&self) -> Result<usize, Self::Error> {
        let node_infos = self.node_infos.borrow();
        Ok(node_infos.values().filter(|info| !info.is_stale()).count())
    }

    async fn all_bootstrap_nodes_len(&self) -> Result<usize, Self::Error> {
        let node_infos = self.node_infos.borrow();
        Ok(node_infos
            .values()
            .filter(|info| info.is_bootstrap() && !info.is_stale())
            .count())
    }

    async fn remove_node_info(&self, id: &ID) -> Result<bool, Self::Error> {
        let mut node_infos = self.node_infos.borrow_mut();
        Ok(node_infos.remove(id).is_some())
    }

    async fn remove_older_than(&self, duration: Duration) -> Result<usize, Self::Error> {
        let mut counter: usize = 0;
        let mut node_infos = self.node_infos.borrow_mut();
        let infos_last_changed = self.node_infos_last_changed.borrow();
        node_infos.retain(|id, _| {
            let last_changed = infos_last_changed
                .get(id)
                .cloned()
                .expect("last_changed is always set when we borrow_mut to store");
            let keep = last_changed > current_timestamp() - duration.as_secs();
            if !keep {
                counter += 1;
            }
            keep
        });
        Ok(counter)
    }

    async fn set_topics(&self, id: ID, topics: HashSet<[u8; 32]>) -> Result<(), Self::Error> {
        self.update_last_changed(id.clone()).await;
        let mut node_topics = self.topics.borrow_mut();
        node_topics.insert(id, HashSet::from_iter(topics.into_iter()));
        Ok(())
    }

    async fn node_infos_by_topics(&self, topics: &[[u8; 32]]) -> Result<Vec<N>, Self::Error> {
        let ids: Vec<ID> = {
            let node_topics = self.topics.borrow();
            node_topics
                .iter()
                .filter_map(|(node_id, node_topics)| {
                    if node_topics.iter().any(|t| topics.contains(t)) {
                        Some(node_id.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

        let node_infos = self.selected_node_infos(ids.as_slice()).await?;

        // Remove stale nodes.
        Ok(node_infos
            .into_iter()
            .filter(|info| !info.is_stale())
            .collect())
    }

    async fn random_node(&self) -> Result<Option<N>, Self::Error> {
        let node_infos = self.node_infos.borrow();
        let mut rng = self.rng.borrow_mut();
        let result = node_infos
            .values()
            .filter(|info| !info.is_stale())
            .choose(&mut *rng);
        Ok(result.cloned())
    }

    async fn random_bootstrap_node(&self) -> Result<Option<N>, Self::Error> {
        let node_infos = self.node_infos.borrow();
        let mut rng = self.rng.borrow_mut();
        let result = node_infos
            .values()
            .filter(|info| info.is_bootstrap() && !info.is_stale())
            .choose(&mut *rng);
        Ok(result.cloned())
    }
}

pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is not behind")
        .as_secs()
}
