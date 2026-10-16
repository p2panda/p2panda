// SPDX-License-Identifier: MIT OR Apache-2.0

//! Waiters which resolve when an matching event is observed.
use std::fmt;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use tokio::sync::oneshot;

use crate::event::{Event, GroupEvent, SpaceEvent};
use crate::member::AuthenticatedMember;

/// Registered handles awaiting a specific event to arrive for a member, group or space.
#[derive(Debug)]
pub(crate) struct EventWaiters<C> {
    pub members: WaitEntries<AuthenticatedMember>,
    pub groups: WaitEntries<GroupEvent<C>>,
    pub spaces: WaitEntries<SpaceEvent<C>>,
}

impl<C> EventWaiters<C> {
    pub fn new() -> Self {
        Self {
            members: WaitEntries::new(),
            groups: WaitEntries::new(),
            spaces: WaitEntries::new(),
        }
    }

    /// Notify all waiters whose predicate matches one of the given events.
    ///
    /// Returns number of notified waiters.
    pub fn notify_events<'a>(&self, events: impl IntoIterator<Item = &'a Event<C>>) -> usize
    where
        C: 'a,
    {
        let mut notified = 0;

        for event in events {
            notified += match event {
                Event::Member(member) => self.members.notify(member),
                Event::Groups(event) => self.groups.notify(event),
                Event::Spaces(event) => self.spaces.notify(event),
                Event::Application { .. } => 0,
            };
        }

        notified
    }
}

type WaitPredicate<E> = Box<dyn Fn(&E) -> bool + Send>;

struct WaitEntry<E> {
    id: u64,
    predicate: WaitPredicate<E>,
    tx: oneshot::Sender<()>,
}

struct WaitEntriesInner<E> {
    next_id: u64,
    entries: Vec<WaitEntry<E>>,
}

/// Set of waiters resolving as soon as a matching event was observed.
pub(crate) struct WaitEntries<E> {
    inner: Arc<Mutex<WaitEntriesInner<E>>>,
}

impl<E> WaitEntries<E> {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(WaitEntriesInner {
                next_id: 0,
                entries: Vec::new(),
            })),
        }
    }

    /// Register a new waiter which resolves when an event matching the predicate is notified.
    pub fn wait_for<P>(&self, predicate: P) -> Waiter<E>
    where
        P: Fn(&E) -> bool + Send + 'static,
    {
        let mut inner = self.inner.lock().expect("lock not poisoned");

        // Unique identifier used for cleaning-up entries when waiter gets dropped.
        let id = inner.next_id;
        inner.next_id += 1;

        let (tx, rx) = oneshot::channel();
        inner.entries.push(WaitEntry {
            id,
            tx,
            predicate: Box::new(predicate),
        });

        Waiter {
            id,
            rx,
            entries: self.clone(),
        }
    }

    /// Notify and remove all waiters whose predicate matches the given event.
    ///
    /// Returns number of notified waiters.
    pub fn notify(&self, event: &E) -> usize {
        let mut inner = self.inner.lock().expect("lock not poisoned");
        let mut notified = 0;

        for entry in inner
            .entries
            .extract_if(.., |entry| (entry.predicate)(event))
        {
            // Receiver might have been dropped already, we don't care.
            let _ = entry.tx.send(());
            notified += 1;
        }

        notified
    }

    /// Remove the entry for the given waiter id (if it wasn't notified yet).
    fn cleanup(&self, id: u64) {
        let mut inner = self.inner.lock().expect("lock not poisoned");
        inner.entries.retain(|entry| entry.id != id);
    }

    fn len(&self) -> usize {
        self.inner.lock().expect("lock not poisoned").entries.len()
    }
}

impl<E> Clone for WaitEntries<E> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<E> fmt::Debug for WaitEntries<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WaitEntries")
            .field("len", &self.len())
            .finish()
    }
}

/// Future which resolves when a matching event was notified.
///
/// Dropping the waiter unregisters it.
pub(crate) struct Waiter<E> {
    id: u64,
    rx: oneshot::Receiver<()>,
    entries: WaitEntries<E>,
}

impl<E> Future for Waiter<E> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Errors only if the sender is dropped, which we treat as done.
        Pin::new(&mut self.get_mut().rx).poll(cx).map(|_| ())
    }
}

impl<E> Drop for Waiter<E> {
    fn drop(&mut self) {
        self.entries.cleanup(self.id);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use p2panda_core::Topic;

    use crate::SpaceId;
    use crate::event::{Event, GroupContext, SpaceContext, SpaceEvent};
    use crate::test_utils::TestConditions;

    use super::EventWaiters;

    fn created_space_event(space_id: SpaceId) -> SpaceEvent<TestConditions> {
        let actor = p2panda_core::SigningKey::from_bytes(&[1; 32]).verifying_key();

        SpaceEvent::Created {
            space_id,
            initial_members: vec![],
            context: SpaceContext {
                author: actor,
                group_id: actor,
                members: vec![],
                actors: vec![],
            },
            groups_context: GroupContext {
                author: actor,
                actors: vec![],
                members: vec![],
                ancestors: vec![],
                effected_group_members: HashMap::default(),
                effected_group_actors: HashMap::default(),
            },
        }
    }

    fn is_created(space_id: SpaceId) -> impl Fn(&SpaceEvent<TestConditions>) -> bool {
        move |event| matches!(event, SpaceEvent::Created { space_id: id, .. } if *id == space_id)
    }

    #[tokio::test]
    async fn notify() {
        let waiters = EventWaiters::<TestConditions>::new();

        let space_id: SpaceId = Topic::random().into();
        let other_space_id: SpaceId = Topic::random().into();

        let waiter = waiters.spaces.wait_for(is_created(space_id));
        let other_waiter = waiters.spaces.wait_for(is_created(other_space_id));
        assert_eq!(waiters.spaces.len(), 2);

        let handle = tokio::spawn(waiter);

        // Unrelated event doesn't do anything.
        assert_eq!(
            waiters
                .spaces
                .notify(&created_space_event(Topic::random().into())),
            0
        );

        // Matching event resolves only the regarding waiter.
        assert_eq!(waiters.spaces.notify(&created_space_event(space_id)), 1);
        tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("waiter resolved")
            .unwrap();
        assert_eq!(waiters.spaces.len(), 1);

        // Dropping waiter unregisters it.
        drop(other_waiter);
        assert_eq!(waiters.spaces.len(), 0);
    }

    #[tokio::test]
    async fn notify_multiple_waiters() {
        let waiters = EventWaiters::<TestConditions>::new();
        let space_id: SpaceId = Topic::random().into();

        let waiter_1 = waiters.spaces.wait_for(is_created(space_id));
        let waiter_2 = waiters.spaces.wait_for(is_created(space_id));

        assert_eq!(waiters.spaces.notify(&created_space_event(space_id)), 2);
        waiter_1.await;
        waiter_2.await;
        assert_eq!(waiters.spaces.len(), 0);
    }

    #[tokio::test]
    async fn notify_all() {
        let waiters = EventWaiters::<TestConditions>::new();
        let space_id: SpaceId = Topic::random().into();

        let waiter = waiters.spaces.wait_for(is_created(space_id));

        let events = [
            Event::Application {
                space_id,
                data: vec![],
            },
            Event::Spaces(created_space_event(space_id)),
        ];

        assert_eq!(waiters.notify_events(&events), 1);
        waiter.await;
    }
}
