// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashSet, VecDeque};
use std::hash::Hash;

pub static DEFAULT_BUFFER_CAPACITY: usize = 10000;

/// Maintain a ring buffer of generic items and efficiently identify if an item is currently in
/// the buffer.
#[derive(Debug)]
pub struct Dedup<T> {
    buffer: VecDeque<T>,
    set: HashSet<T>,
}

impl<T> Default for Dedup<T> {
    fn default() -> Self {
        Self {
            buffer: VecDeque::with_capacity(DEFAULT_BUFFER_CAPACITY),
            set: HashSet::with_capacity(DEFAULT_BUFFER_CAPACITY),
        }
    }
}

impl<T> Dedup<T>
where
    T: Eq + Hash + Clone,
{
    /// Instantiate a new buffer with "capacity" buffer size.
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            set: HashSet::with_capacity(capacity),
        }
    }

    /// Insert an item into the buffer.
    ///
    /// If the buffer capacity has been reached then the oldest item will be evicted from the
    /// buffer.
    ///
    /// Returns `true` if an insertion occurred, or `false` if the item was already in the buffer.
    pub fn insert(&mut self, item: T) -> bool {
        if self.set.contains(&item) {
            return false;
        }

        // If the buffer is at max capacity then we first need to pop an item from the front and
        // remove it from the set.
        if self.buffer.len() + 1 > self.buffer.capacity() {
            let evicted = self.buffer.pop_front();
            if let Some(evicted) = evicted {
                println!("evicted");
                self.set.remove(&evicted);
            }
        }

        self.buffer.push_back(item.clone());
        self.set.insert(item);
        true
    }

    // Returns `true` if the item is currently in the buffer.
    pub fn contains(&self, item: &T) -> bool {
        self.set.contains(item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_items() {
        let mut d = Dedup::new(3);

        assert!(d.insert(1));
        assert!(d.insert(2));
        assert!(d.insert(3));

        assert!(d.contains(&1));
        assert!(d.contains(&2));
        assert!(d.contains(&3));
    }

    #[test]
    fn insert_ignores_duplicates() {
        let mut d = Dedup::new(3);

        assert!(d.insert(42));
        assert!(!d.insert(42));

        assert!(d.contains(&42));
        assert_eq!(d.buffer.len(), 1);
        assert_eq!(d.set.len(), 1);
    }

    #[test]
    fn insert_evicts_when_capacity_reached() {
        let mut d = Dedup::new(3);

        d.insert(1);
        d.insert(2);
        d.insert(3);
        assert!(d.contains(&1));

        d.insert(4);

        assert!(!d.contains(&1));
        assert!(d.contains(&2));
        assert!(d.contains(&3));
        assert!(d.contains(&4));

        assert_eq!(d.buffer.len(), 3);
    }
}
