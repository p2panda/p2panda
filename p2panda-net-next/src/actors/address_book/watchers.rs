// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash as StdHash;

use tokio::sync::mpsc;

/// When set "false" the subscription will directly yield the current state without waiting for
/// changes to come.
pub type UpdatesOnly = bool;

pub trait Watched {
    type Value: Clone;

    /// Returns the current watched value state.
    fn current(&self) -> Self::Value;

    /// Calculates the difference between this and another value and updates it if value changed.
    fn update_if_changed(&self, cmp: &Self::Value) -> UpdateResult<Self::Value>;
}

pub enum UpdateResult<V>
where
    V: Clone,
{
    /// No difference was given between this and another value.
    Unchanged,

    /// The "new" value changed and we're including the difference (between the old and new value)
    /// and the new value itself.
    Changed(WatchedValue<V>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct WatchedValue<V>
where
    V: Clone,
{
    // Not all values can be used to compute a difference, this is why this is optional.
    pub difference: Option<V>,
    pub value: V,
}

pub type WatcherSender<V> = mpsc::UnboundedSender<WatchedValue<V>>;

pub type WatcherReceiver<V> = mpsc::UnboundedReceiver<WatchedValue<V>>;

pub struct Watcher<T>
where
    T: Watched,
{
    watched: T,
    // We're _not_ using a broadcast channel here since we don't want to notify _all_ subscribers
    // when _some_ of them are interested in updates only and others are interested in also
    // receiving the current value right after subscribing.
    subscribers: RefCell<Vec<WatcherSender<T::Value>>>,
}

impl<T> Watcher<T>
where
    T: Watched,
{
    pub fn new(initial: T) -> Self {
        Self {
            watched: initial,
            subscribers: RefCell::new(Vec::new()),
        }
    }

    pub fn update(&self, value: T::Value) {
        if let UpdateResult::Changed(result) = self.watched.update_if_changed(&value) {
            // If the value has changed we inform all subscribers.
            self.notify(result);
        }
    }

    pub fn subscribe(&self, updates_only: UpdatesOnly) -> WatcherReceiver<T::Value> {
        let (tx, rx) = mpsc::unbounded_channel();

        // Immediately send watched value if subscriber is not only interested in updates.
        if !updates_only {
            // Ignore send error here since we're still holding onto the receiver.
            let _ = tx.send(WatchedValue {
                // The difference _is_ the same as the current value. This is from the perspective
                // of a subscriber "seeing" it for the "first time", before they had "nothing".
                difference: Some(self.watched.current()),
                value: self.watched.current(),
            });
        }

        // Remember subscriber for later notifications.
        let mut subscribers = self.subscribers.borrow_mut();
        subscribers.push(tx);

        rx
    }

    pub fn len(&self) -> usize {
        self.subscribers.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.subscribers.borrow().is_empty()
    }

    fn notify(&self, value: WatchedValue<T::Value>) {
        let mut subscribers = self.subscribers.borrow_mut();

        // Remove subscribers automatically if they've dropped the receiver's channel end.
        subscribers.retain(|tx| tx.send(value.clone()).is_ok());
    }
}

pub struct WatcherSet<K, T>
where
    T: Watched,
{
    watchers: RefCell<HashMap<K, Watcher<T>>>,
}

impl<K, T> WatcherSet<K, T>
where
    K: Eq + StdHash,
    T: Watched,
{
    pub fn new() -> Self {
        Self {
            watchers: RefCell::new(HashMap::new()),
        }
    }

    pub fn subscribe(
        &self,
        key: K,
        updates_only: UpdatesOnly,
        initial: T,
    ) -> WatcherReceiver<T::Value> {
        let mut watchers = self.watchers.borrow_mut();
        if watchers.contains_key(&key) {
            let watcher = watchers.get_mut(&key).expect("we've checked it exists");
            watcher.subscribe(updates_only)
        } else {
            let watcher = Watcher::new(initial);
            let rx = watcher.subscribe(updates_only);
            watchers.insert(key, watcher);
            rx
        }
    }

    pub fn update(&self, key: &K, value: T::Value) {
        let mut watchers = self.watchers.borrow_mut();

        // Check if anyone is interested in this update and inform them.
        if let Some(watcher) = watchers.get_mut(key) {
            watcher.update(value);

            // Clean up watcher if there's no subscribers left for that key.
            if watcher.is_empty() {
                watchers.remove(&key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashSet;

    use tokio::sync::mpsc::error::TryRecvError;

    use super::{UpdateResult, Watched, WatchedValue, Watcher};

    #[test]
    fn subscribe_to_changes() {
        struct WatchedSet(RefCell<HashSet<u64>>);

        impl WatchedSet {
            pub fn new(set: HashSet<u64>) -> Self {
                Self(RefCell::new(set))
            }
        }

        impl Watched for WatchedSet {
            type Value = HashSet<u64>;

            fn current(&self) -> Self::Value {
                self.0.borrow().clone()
            }

            fn update_if_changed(&self, cmp: &Self::Value) -> UpdateResult<Self::Value> {
                let difference: HashSet<u64> =
                    self.0.borrow().symmetric_difference(cmp).cloned().collect();

                if difference.is_empty() {
                    UpdateResult::Unchanged
                } else {
                    self.0.replace(cmp.to_owned());

                    UpdateResult::Changed(WatchedValue {
                        difference: Some(difference),
                        value: cmp.to_owned(),
                    })
                }
            }
        }

        let set = WatchedSet::new(HashSet::from_iter([1, 2, 3]));
        let watcher = Watcher::new(set);

        let mut updates_only_rx = watcher.subscribe(true);
        let mut rx = watcher.subscribe(false);

        // Subscriber doesn't receive an item right at the beginning as they are only interested in
        // "updates".
        assert!(matches!(
            updates_only_rx.try_recv(),
            Err(TryRecvError::Empty)
        ));

        // Second subscriber was interested in the current value and directly receives it.
        let result = rx.try_recv().expect("should return Ok");
        assert_eq!(result.value, HashSet::from_iter([1, 2, 3]),);

        // Difference is the current value at the beginning.
        assert_eq!(result.difference, Some(result.value));

        // Value gets updated, but nothing has changed.
        watcher.update(HashSet::from_iter([1, 2, 3]));

        // Subscribers do not get notified.
        assert!(matches!(
            updates_only_rx.try_recv(),
            Err(TryRecvError::Empty)
        ));
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));

        // Value gets updated, this time with a real change.
        watcher.update(HashSet::from_iter([1, 2, 3, 4]));

        // Everyone gets notified.
        let result_1 = rx.try_recv().expect("should return Ok");
        let result_2 = updates_only_rx.try_recv().expect("should return Ok");
        assert_eq!(result_1, result_2);
        assert_eq!(result_1.value, HashSet::from_iter([1, 2, 3, 4]),);
        assert_eq!(result_1.difference, Some(HashSet::from_iter([4])));

        // Value gets updated again.
        watcher.update(HashSet::from_iter([1, 2, 3]));

        // Everyone gets notified.
        let result_1 = rx.try_recv().expect("should return Ok");
        let result_2 = updates_only_rx.try_recv().expect("should return Ok");
        assert_eq!(result_1, result_2);
        assert_eq!(result_1.value, HashSet::from_iter([1, 2, 3]),);
        assert_eq!(result_1.difference, Some(HashSet::from_iter([4])));
    }
}
