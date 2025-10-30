use ringbuffer::{AllocRingBuffer, RingBuffer};
use std::collections::HashSet;
use std::hash::Hash;

pub static DEFAULT_BUFFER_CAPACITY: usize = 10000;

#[derive(Debug)]
pub struct Dedup<T> {
    buffer: AllocRingBuffer<T>,
    set: HashSet<T>,
}

impl<T> Default for Dedup<T> {
    fn default() -> Self {
        Self {
            buffer: AllocRingBuffer::new(DEFAULT_BUFFER_CAPACITY),
            set: HashSet::with_capacity(DEFAULT_BUFFER_CAPACITY),
        }
    }
}

impl<T> Dedup<T>
where
    T: Eq + Hash + Clone,
{
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: AllocRingBuffer::new(capacity),
            set: HashSet::with_capacity(capacity),
        }
    }

    pub fn insert(&mut self, item: T) -> bool {
        if self.set.contains(&item) {
            return false;
        }

        let evicted = self.buffer.enqueue(item.clone());

        if let Some(ref removed) = evicted {
            self.set.remove(removed);
        }

        self.set.insert(item);
        true
    }

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
