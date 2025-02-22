// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash as StdHash;

#[derive(Debug)]
pub struct DependencyChecker<K, V> {
    processed: HashSet<K>,
    pending_queue: HashMap<K, Vec<(K, V, Vec<K>)>>,
    ready_queue: VecDeque<V>,
}

impl<K, V> DependencyChecker<K, V>
where
    K: Clone + Copy + StdHash + PartialEq + Eq,
    V: Clone + StdHash + PartialEq + Eq,
{
    pub fn new() -> Self {
        Self {
            processed: HashSet::new(),
            pending_queue: HashMap::new(),
            ready_queue: VecDeque::new(),
        }
    }

    pub fn next(&mut self) -> Option<V> {
        self.ready_queue.pop_front()
    }

    pub fn process(&mut self, key: K, value: V, dependencies: Vec<K>) {
        let mut deps_met = true;

        // For all dependencies of this item, check if they have been processed already, if not
        // add a new item to the pending_queue for each.
        for dependency in &dependencies {
            if !self.processed.contains(dependency) {
                deps_met = false;
                let dependents = self.pending_queue.entry(*dependency).or_default();
                dependents.push((key, value.clone(), dependencies.clone()));
            }
        }

        if !deps_met {
            // If any of the dependencies were not met return now.
            return;
        }

        // From this point we know the item we are processing has all it's dependencies met, so
        // insert it's key into the processed set.
        self.processed.insert(key);

        // And move it to the ready queue.
        self.ready_queue.push_back(value);

        self.process_pending(key);
    }

    /// Recursively check if any pending items now have their dependencies met (due to another
    // item being processed).
    fn process_pending(&mut self, key: K) {
        // Take the entry at key from the pending_queue, the value contains all items which depend
        // on this item as one of their dependencies.
        if let Some((_, dependents)) = self.pending_queue.remove_entry(&key) {
            for (dependent_key, dependent_value, dependencies) in dependents {
                let dependencies = HashSet::from_iter(dependencies.iter().cloned());

                // Check if all the dependencies are now met.
                if self.processed.is_superset(&dependencies) {
                    // If so add an entry to the processed set.
                    self.processed.insert(dependent_key);

                    // And insert this value to the ready_queue.
                    self.ready_queue.push_back(dependent_value);

                    // Now check if this item moving into the processed set results in any other
                    // items having all their dependencies met.
                    self.process_pending(dependent_key);
                }
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::DependencyChecker;

    #[test]
    fn dependency_check() {
        // A has no dependencies and so it's added straight to the processed set and ready queue.
        let mut checker = DependencyChecker::new();
        checker.process("a", "A", vec![]);
        assert!(checker.processed.len() == 1);
        assert!(checker.pending_queue.is_empty());
        assert_eq!(checker.ready_queue.len(), 1);

        // B has it's dependencies met and so it too is added to the processed set and ready queue.
        checker.process("b", "B", vec!["a"]);
        assert!(checker.processed.len() == 2);
        assert!(checker.pending_queue.is_empty());
        assert_eq!(checker.ready_queue.len(), 2);

        // D doesn't have both its dependencies met yet so it waits in the pending queue.
        checker.process("d", "D", vec!["b", "c"]);
        assert_eq!(checker.processed.len(), 2);
        assert_eq!(checker.pending_queue.len(), 1);
        assert_eq!(checker.ready_queue.len(), 2);

        // C satisfies D's dependencies and so both C & D are added to the processed set
        // and ready queue.
        checker.process("c", "C", vec!["b"]);
        assert_eq!(checker.processed.len(), 4);
        assert!(checker.pending_queue.is_empty());
        assert_eq!(checker.ready_queue.len(), 4);

        let item = checker.next();
        assert_eq!(item, Some("A"));
        let item = checker.next();
        assert_eq!(item, Some("B"));
        let item = checker.next();
        assert_eq!(item, Some("C"));
        let item = checker.next();
        assert_eq!(item, Some("D"));
        let item = checker.next();
        assert!(item.is_none());
    }
}
