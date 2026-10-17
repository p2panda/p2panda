// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::marker::PhantomData;

use p2panda_auth::group;
use p2panda_auth::traits::{Conditions, Operation as AuthOperationTrait};
use p2panda_core::{Hash, VerifyingKey};
use p2panda_spaces::AuthMessage;
use p2panda_store::Transaction;
use p2panda_store::groups::GroupsStore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Notify;
use tracing::debug;

use crate::Processor;
use crate::groups::GroupsProcessorArgs;

type GroupsCrdt<C> = group::GroupCrdt<VerifyingKey, Hash, AuthMessage<C>, C, StrongRemove<C>>;

type StrongRemove<C> = group::resolver::StrongRemove<VerifyingKey, Hash, AuthMessage<C>, C>;

#[derive(Clone, Debug)]
pub enum GroupsResult {
    Processed,
    Noop,
}

impl GroupsResult {
    pub fn was_processed(self) -> bool {
        match self {
            Self::Processed => true,
            Self::Noop => false,
        }
    }
}

/// Processor for groups operations.
pub struct Groups<S, T, C> {
    store: S,
    state_id: Hash,
    notify: Notify,
    queue: RefCell<VecDeque<(T, GroupsResult)>>,
    _marker: PhantomData<C>,
}

impl<S, T, C> Groups<S, T, C>
where
    C: Conditions + Serialize + for<'a> Deserialize<'a>,
{
    pub fn new(store: S, state_id: Hash) -> Self {
        Self {
            store,
            state_id,
            notify: Notify::new(),
            queue: RefCell::new(VecDeque::new()),
            _marker: PhantomData,
        }
    }
}

impl<S, T, C> Processor<T> for Groups<S, T, C>
where
    S: GroupsStore<AuthMessage<C>, C> + Transaction,
    T: Borrow<GroupsProcessorArgs<C>>,
    C: Conditions + Serialize + for<'a> Deserialize<'a>,
{
    type Output = (T, GroupsResult);

    type Error = (T, GroupsError);

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        let input_args: &GroupsProcessorArgs<C> = input.borrow();

        // Extract GroupArgs from the extension headers of an Operation<E>.
        //
        // If this returns None then the groups extension was not present and we consider this a
        // non-groups operation which does not require processing.
        let result = if let GroupsProcessorArgs::Process { message } = input_args {
            let permit = match self.store.begin().await {
                Ok(permit) => permit,
                Err(err) => return Err((input, GroupsError::Store(err.to_string()))),
            };

            let mut y = match GroupsStore::<AuthMessage<C>, C>::get_groups_state_tx(
                &self.store,
                self.state_id,
            )
            .await
            {
                Err(err) => return Err((input, GroupsError::Store(err.to_string()))),
                Ok(Some(y)) => y,
                Ok(None) => Default::default(),
            };

            debug!(
                group_id = %message.group_id(),
                "current group membership: {:?}",
                y.members(message.group_id())
            );

            debug!(id = %message.operation_id, "apply message to group state");

            y = match GroupsCrdt::process(y, message) {
                Ok(y) => y,
                Err(err) => return Err((input, GroupsError::Groups(err.to_string()))),
            };

            if let Err(err) = self.store.set_groups_state_tx(self.state_id, &y).await {
                return Err((input, GroupsError::Store(err.to_string())));
            }

            if let Err(err) = self.store.commit(permit).await {
                return Err((input, GroupsError::Store(err.to_string())));
            }

            debug!(
                group_id = %message.group_id(),
                "new group membership: {:?}",
                y.members(message.group_id())
            );

            (input, GroupsResult::Processed)
        } else {
            (input, GroupsResult::Noop)
        };

        self.queue.borrow_mut().push_back(result);
        self.notify.notify_one(); // Wake up any pending recv.

        Ok(())
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        loop {
            if let Some(item) = self.queue.borrow_mut().pop_front() {
                return Ok(item);
            }

            // Wait for notification that an item was added.
            self.notify.notified().await;
        }
    }
}

/// Error types which can occur in the groups processor.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Error)]
pub enum GroupsError {
    /// Critical storage failure occurred. This is usually a reason to panic.
    #[error("critical storage failure: {0}")]
    Store(String),

    #[error("error processing groups operation: {0}")]
    Groups(String),
}

#[cfg(test)]
mod tests {
    use std::borrow::Borrow;

    use p2panda_auth::group::{GroupAction, GroupCrdtState, GroupMember};
    use p2panda_auth::{Access, GroupsExtensionArgs};
    use p2panda_core::test_utils::{TestLog, setup_logging};
    use p2panda_core::traits::{Digest, Provenance};
    use p2panda_core::{Hash, Operation, SigningKey, Topic, VerifyingKey};
    use p2panda_spaces::AuthMessage;
    use p2panda_store::groups::GroupsStore;
    use p2panda_store::{SqliteStore, Transaction, tx_unwrap};
    use serde::{Deserialize, Serialize};

    use crate::Processor;
    use crate::groups::GroupsProcessorArgs;
    use crate::ingest::{Ingest, IngestArgs};

    type LogId = usize;

    type GroupsState = GroupCrdtState<VerifyingKey, Hash, AuthMessage<()>, ()>;

    type Groups = crate::groups::Groups<SqliteStore, Event, ()>;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestExtensions {
        log_id: LogId,
        dependencies: Vec<Hash>,
        groups: Option<GroupsExtensionArgs>,
    }

    impl From<GroupsExtensionArgs> for TestExtensions {
        fn from(args: GroupsExtensionArgs) -> Self {
            TestExtensions {
                log_id: 0,
                dependencies: args.dependencies.clone(),
                groups: Some(args),
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Event {
        pub operation: Operation<TestExtensions>,
        pub ingest_args: IngestArgs<LogId, Topic>,
        pub groups_args: GroupsProcessorArgs<()>,
    }

    impl From<Operation<TestExtensions>> for Event {
        fn from(operation: Operation<TestExtensions>) -> Self {
            Self {
                ingest_args: IngestArgs {
                    log_id: operation.header.extensions.log_id,
                    topic: Hash::digest("test").into(),
                    prune_flag: false,
                },
                groups_args: match operation.header.extensions.groups {
                    Some(ref groups) => GroupsProcessorArgs::Process {
                        message: AuthMessage {
                            operation_id: operation.hash(),
                            author: operation.author(),
                            dependencies: operation.header.extensions.dependencies.clone(),
                            group_id: groups.group_id,
                            action: groups.action.clone(),
                        },
                    },
                    None => GroupsProcessorArgs::Ignore,
                },
                operation,
            }
        }
    }

    // Ingest

    impl Borrow<IngestArgs<LogId, Topic>> for Event {
        fn borrow(&self) -> &IngestArgs<LogId, Topic> {
            &self.ingest_args
        }
    }

    impl Borrow<Operation<TestExtensions>> for Event {
        fn borrow(&self) -> &Operation<TestExtensions> {
            &self.operation
        }
    }

    // Groups

    impl Borrow<GroupsProcessorArgs<()>> for Event {
        fn borrow(&self) -> &GroupsProcessorArgs<()> {
            &self.groups_args
        }
    }

    #[tokio::test]
    async fn basic_processing() {
        setup_logging();

        let store = SqliteStore::temporary().await;

        let state_id = Hash::digest(b"default");
        let group_id = SigningKey::generate().verifying_key();

        let alice_log = TestLog::new();
        let bobby_log = TestLog::new();

        let alice = alice_log.author();
        let bobby = bobby_log.author();

        let args = GroupsExtensionArgs {
            group_id,
            action: GroupAction::Create {
                initial_members: vec![
                    (GroupMember::Individual(alice), <Access>::manage()),
                    (GroupMember::Individual(bobby), <Access>::manage()),
                ],
            },
            dependencies: vec![],
        };

        let operation: Operation<TestExtensions> =
            alice_log.operation(&[], TestExtensions::from(args.clone()));

        let event = Event::from(operation);

        // Operation needs to be ingested before it can be processed by the groups processor.
        let ingest = Ingest::new(store.clone());
        ingest.process(event.clone()).await.unwrap();

        let groups = Groups::new(store.clone(), Hash::digest("default"));
        groups.process(event).await.unwrap();

        let (_processed_op, result) = groups.next().await.unwrap();
        assert!(result.was_processed());

        let permit = store.begin().await.unwrap();
        let y: GroupsState = store.get_groups_state_tx(state_id).await.unwrap().unwrap();
        store.commit(permit).await.unwrap();

        let members = y.members(group_id);
        assert_eq!(members.len(), 2);
        assert!(members.contains(&(alice, Access::manage())));
        assert!(members.contains(&(bobby, Access::manage())));
    }

    #[tokio::test]
    async fn device_groups_single_context() {
        let alice_log = TestLog::new();
        let bobby_log = TestLog::new();
        let cathy_log = TestLog::new();

        let alice = alice_log.author();
        let bobby = bobby_log.author();
        let cathy = cathy_log.author();

        let alice_store = SqliteStore::temporary().await;
        let bobby_store = SqliteStore::temporary().await;
        let cathy_store = SqliteStore::temporary().await;

        let alice_device_group = SigningKey::generate().verifying_key();
        let bobby_device_group = SigningKey::generate().verifying_key();
        let cathy_device_group = SigningKey::generate().verifying_key();

        let ab_chat = SigningKey::generate().verifying_key();
        let bc_chat = SigningKey::generate().verifying_key();

        let alice_ingest = Ingest::new(alice_store.clone());
        let bobby_ingest = Ingest::new(bobby_store.clone());
        let cathy_ingest = Ingest::new(cathy_store.clone());

        let alice_groups = Groups::new(alice_store.clone(), Hash::digest("default"));
        let bobby_groups = Groups::new(bobby_store.clone(), Hash::digest("default"));
        let cathy_groups = Groups::new(cathy_store.clone(), Hash::digest("default"));

        // All members create their own device groups and process them on their own stores.

        let args = GroupsExtensionArgs {
            group_id: alice_device_group,
            action: GroupAction::Create {
                initial_members: vec![(GroupMember::Individual(alice), Access::manage())],
            },
            dependencies: vec![],
        };

        let create_alice_device_00: Operation<TestExtensions> =
            alice_log.operation(&[], TestExtensions::from(args));

        let event_00 = Event::from(create_alice_device_00.clone());

        alice_ingest.process(event_00.clone()).await.unwrap();
        alice_groups.process(event_00).await.unwrap();

        let args = GroupsExtensionArgs {
            group_id: bobby_device_group,
            action: GroupAction::Create {
                initial_members: vec![(GroupMember::Individual(bobby), Access::manage())],
            },
            dependencies: vec![],
        };
        let create_bobby_device_01: Operation<TestExtensions> =
            bobby_log.operation(&[], TestExtensions::from(args));
        let event_01 = Event::from(create_bobby_device_01.clone());

        bobby_ingest.process(event_01.clone()).await.unwrap();
        bobby_groups.process(event_01.clone()).await.unwrap();

        let args = GroupsExtensionArgs {
            group_id: cathy_device_group,
            action: GroupAction::Create {
                initial_members: vec![(GroupMember::Individual(cathy), Access::manage())],
            },
            dependencies: vec![],
        };
        let create_cathy_device_02: Operation<TestExtensions> =
            cathy_log.operation(&[], TestExtensions::from(args));
        let event_02 = Event::from(create_cathy_device_02.clone());

        cathy_ingest.process(event_02.clone()).await.unwrap();
        cathy_groups.process(event_02).await.unwrap();

        // Alice creates chat with Bobby.
        //
        // First they process "create device group" operation from Bobby.
        alice_ingest.process(event_01.clone()).await.unwrap();
        alice_groups.process(event_01.clone()).await.unwrap();

        // Then they create the chat group.
        let y: GroupsState = tx_unwrap!(alice_store, {
            let state_id = Hash::digest("default");
            alice_store
                .get_groups_state_tx(state_id)
                .await
                .unwrap()
                .unwrap()
        });

        let args = GroupsExtensionArgs {
            group_id: ab_chat,
            action: GroupAction::Create {
                initial_members: vec![
                    (GroupMember::Group(alice_device_group), Access::write()),
                    (GroupMember::Group(bobby_device_group), Access::write()),
                ],
            },
            dependencies: y.heads(&[alice_device_group, bobby_device_group]),
        };
        let create_alice_bobby_chat_03: Operation<TestExtensions> =
            alice_log.operation(&[], TestExtensions::from(args));
        let event_03 = Event::from(create_alice_bobby_chat_03.clone());

        alice_ingest.process(event_03.clone()).await.unwrap();
        alice_groups.process(event_03).await.unwrap();

        // Bobby processes alice's "create device group" and "create ab chat".
        for op in [create_alice_device_00.clone(), create_alice_bobby_chat_03] {
            let event: Event = op.into();

            bobby_ingest.process(event.clone()).await.unwrap();
            bobby_groups.process(event).await.unwrap();
        }

        // Both Alice and Bobby have the correct groups state.
        for store in [alice_store.clone(), bobby_store.clone()] {
            let y: GroupsState = tx_unwrap!(store, {
                let state_id = Hash::digest("default");
                store.get_groups_state_tx(state_id).await.unwrap().unwrap()
            });

            let mut members = y.members(ab_chat);
            members.sort();

            assert_eq!(members.len(), 2);
            assert!(members.contains(&(alice, Access::write())));
            assert!(members.contains(&(bobby, Access::write())));
        }

        // Cathy now creates a chat with Bobby.
        //
        // First they process "create device group" for bobby.
        cathy_ingest.process(event_01.clone()).await.unwrap();
        cathy_groups.process(event_01).await.unwrap();

        // Then they create the chat group.
        let y: GroupsState = tx_unwrap!(cathy_store, {
            let state_id = Hash::digest("default");
            cathy_store
                .get_groups_state_tx(state_id)
                .await
                .unwrap()
                .unwrap()
        });

        let args = GroupsExtensionArgs {
            group_id: bc_chat,
            action: GroupAction::Create {
                initial_members: vec![
                    (GroupMember::Group(bobby_device_group), Access::write()),
                    (GroupMember::Group(cathy_device_group), Access::write()),
                ],
            },
            dependencies: y.heads(&[bobby_device_group, cathy_device_group]),
        };
        let create_bobby_cathy_chat_04: Operation<TestExtensions> =
            cathy_log.operation(&[], TestExtensions::from(args));
        let event_04 = Event::from(create_bobby_cathy_chat_04.clone());

        cathy_ingest.process(event_04.clone()).await.unwrap();
        cathy_groups.process(event_04).await.unwrap();

        // Bobby processes cathy's "create device group" and "create bc chat".
        for op in [create_cathy_device_02.clone(), create_bobby_cathy_chat_04] {
            let event: Event = op.into();

            bobby_ingest.process(event.clone()).await.unwrap();
            bobby_groups.process(event).await.unwrap();
        }

        // Both Cathy and Bobby have the correct groups state.
        for store in [cathy_store, bobby_store] {
            let y: GroupsState = tx_unwrap!(store, {
                let state_id = Hash::digest("default");
                store.get_groups_state_tx(state_id).await.unwrap().unwrap()
            });
            let mut members = y.members(bc_chat);
            members.sort();

            assert_eq!(members.len(), 2);
            assert!(members.contains(&(bobby, Access::write())));
            assert!(members.contains(&(cathy, Access::write())));
        }
    }
}
