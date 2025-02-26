// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash as StdHash;

// @TODO: eventually processed and pending_queue needs to be populated from the database.
#[derive(Debug)]
pub struct DependencyChecker<K> {
    /// Keys for all items we have processed.
    /// @TODO: store all items which are _not_ processed instead.
    processed: HashSet<K>,

    /// Map of missing dependencies `K` to all items which directly depend on them `(K, V, Vec<K>)`.
    pending_queue: HashMap<K, Vec<(K, Vec<K>)>>,

    /// Queue of items whore dependencies are met. These are returned from calls to `next()`.
    ready_queue: VecDeque<K>,
}

impl<K> DependencyChecker<K>
where
    K: Clone + Copy + StdHash + PartialEq + Eq,
{
    pub fn new() -> Self {
        Self {
            processed: HashSet::new(),
            pending_queue: HashMap::new(),
            ready_queue: VecDeque::new(),
        }
    }

    pub fn next(&mut self) -> Option<K> {
        self.ready_queue.pop_front()
    }

    pub fn process(&mut self, key: K, dependencies: Vec<K>) {
        let mut deps_met = true;

        // For all dependencies of this item, check if they have been processed already, if not
        // add a new item to the pending_queue for each.
        for dependency in &dependencies {
            if !self.processed.contains(dependency) {
                deps_met = false;
                let dependents = self.pending_queue.entry(*dependency).or_default();
                dependents.push((key, dependencies.clone()));
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
        self.ready_queue.push_back(key);

        self.process_pending(key);
    }

    // Recursively check if any pending items now have their dependencies met (due to another
    // item being processed).
    fn process_pending(&mut self, key: K) {
        // Take the entry at key from the pending_queue, the value contains all items which depend
        // on this item as one of their dependencies.
        if let Some((_, dependents)) = self.pending_queue.remove_entry(&key) {
            for (dependent_key, dependencies) in dependents {
                let dependencies = HashSet::from_iter(dependencies.iter().cloned());

                // Check if all the dependencies are now met.
                if self.processed.is_superset(&dependencies) {
                    // If so add an entry to the processed set.
                    self.processed.insert(dependent_key);

                    // And insert this value to the ready_queue.
                    self.ready_queue.push_back(dependent_key);

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
        checker.process("a", vec![]);
        assert!(checker.processed.len() == 1);
        assert!(checker.pending_queue.is_empty());
        assert_eq!(checker.ready_queue.len(), 1);

        // B has it's dependencies met and so it too is added to the processed set and ready queue.
        checker.process("b", vec!["a"]);
        assert!(checker.processed.len() == 2);
        assert!(checker.pending_queue.is_empty());
        assert_eq!(checker.ready_queue.len(), 2);

        // D doesn't have both its dependencies met yet so it waits in the pending queue.
        checker.process("d", vec!["b", "c"]);
        assert_eq!(checker.processed.len(), 2);
        assert_eq!(checker.pending_queue.len(), 1);
        assert_eq!(checker.ready_queue.len(), 2);

        // C satisfies D's dependencies and so both C & D are added to the processed set
        // and ready queue.
        checker.process("c", vec!["b"]);
        assert_eq!(checker.processed.len(), 4);
        assert!(checker.pending_queue.is_empty());
        assert_eq!(checker.ready_queue.len(), 4);

        let item = checker.next();
        assert_eq!(item, Some("a"));
        let item = checker.next();
        assert_eq!(item, Some("b"));
        let item = checker.next();
        assert_eq!(item, Some("c"));
        let item = checker.next();
        assert_eq!(item, Some("d"));
        let item = checker.next();
        assert!(item.is_none());
    }

    #[test]
    fn recursive_dependency_check() {
        let incomplete_graph = [
            ("a", vec![]),
            ("c", vec!["b"]),
            ("d", vec!["c"]),
            ("e", vec!["d"]),
            ("f", vec!["e"]),
            ("g", vec!["f"]),
        ];

        let mut checker = DependencyChecker::new();
        for (key, dependencies) in incomplete_graph {
            checker.process(key, dependencies);
        }
        assert!(checker.processed.len() == 1);
        assert_eq!(checker.pending_queue.len(), 5);
        assert_eq!(checker.ready_queue.len(), 1);

        let missing_dependency = ("b", vec!["a"]);

        checker.process(missing_dependency.0, missing_dependency.1);
        assert!(checker.processed.len() == 7);
        assert_eq!(checker.pending_queue.len(), 0);
        assert_eq!(checker.ready_queue.len(), 7);

        let item = checker.next();
        assert_eq!(item, Some("a"));
        let item = checker.next();
        assert_eq!(item, Some("b"));
        let item = checker.next();
        assert_eq!(item, Some("c"));
        let item = checker.next();
        assert_eq!(item, Some("d"));
        let item = checker.next();
        assert_eq!(item, Some("e"));
        let item = checker.next();
        assert_eq!(item, Some("f"));
        let item = checker.next();
        assert_eq!(item, Some("g"));
        let item = checker.next();
        assert!(item.is_none());
    }

    #[test]
    fn complex_graph() {
        // A <-- B1 <-- C1 <--\
        //   \-- B2 <-- C2 <-- D
        //        \---- C3 <--/
        let incomplete_graph = [
            ("a", vec![]),
            ("b1", vec!["a"]),
            ("c1", vec!["b1"]),
            ("c2", vec!["b2"]),
            ("c3", vec!["b2"]),
            ("d", vec!["c1", "c2", "c3"]),
        ];

        let mut checker = DependencyChecker::new();
        for (key, dependencies) in incomplete_graph {
            checker.process(key, dependencies);
        }

        // A1, B1 and C1 have dependencies met and were already processed.
        assert!(checker.processed.len() == 3);
        assert_eq!(checker.pending_queue.len(), 3);
        assert_eq!(checker.ready_queue.len(), 3);

        let item = checker.next();
        assert_eq!(item, Some("a"));
        let item = checker.next();
        assert_eq!(item, Some("b1"));
        let item = checker.next();
        assert_eq!(item, Some("c1"));
        let item = checker.next();
        assert!(item.is_none());

        // No more ready items.
        assert_eq!(checker.ready_queue.len(), 0);

        // Process the missing item.
        let missing_dependency = ("b2", vec!["a"]);
        checker.process(missing_dependency.0, missing_dependency.1);

        // All items have now been processed and new ones are waiting in the ready queue.
        assert_eq!(checker.processed.len(), 7);
        assert_eq!(checker.pending_queue.len(), 0);
        assert_eq!(checker.ready_queue.len(), 4);

        let item = checker.next();
        assert_eq!(item, Some("b2"));
        let item = checker.next();
        assert_eq!(item, Some("c2"));
        let item = checker.next();
        assert_eq!(item, Some("c3"));
        let item = checker.next();
        assert_eq!(item, Some("d"));
        let item = checker.next();
        assert!(item.is_none());
    }

    #[test]
    fn very_out_of_order() {
        // A <-- B1 <-- C1 <--\
        //   \-- B2 <-- C2 <-- D
        //        \---- C3 <--/
        let out_of_order_graph = [
            ("d", vec!["c1", "c2", "c3"]),
            ("c1", vec!["b1"]),
            ("b1", vec!["a"]),
            ("b2", vec!["a"]),
            ("c3", vec!["b2"]),
            ("c2", vec!["b2"]),
            ("a", vec![]),
        ];

        let mut checker = DependencyChecker::new();
        for (key, dependencies) in out_of_order_graph {
            checker.process(key, dependencies);
        }

        assert!(checker.processed.len() == 7);
        assert_eq!(checker.pending_queue.len(), 0);
        assert_eq!(checker.ready_queue.len(), 7);

        let item = checker.next();
        assert_eq!(item, Some("a"));
        let item = checker.next();
        assert_eq!(item, Some("b1"));
        let item = checker.next();
        assert_eq!(item, Some("c1"));
        let item = checker.next();
        assert_eq!(item, Some("b2"));
        let item = checker.next();
        assert_eq!(item, Some("c3"));
        let item = checker.next();
        assert_eq!(item, Some("c2"));
        let item = checker.next();
        assert_eq!(item, Some("d"));
        let item = checker.next();
        assert!(item.is_none());
    }
}
