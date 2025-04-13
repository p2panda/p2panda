// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils {
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::marker::PhantomData;

    use serde::{Deserialize, Serialize};
    use thiserror::Error;

    use crate::message_scheme::test_utils::{MemberId, MessageId};
    use crate::message_scheme::{ControlMessage, DirectMessage, Generation};
    use crate::traits::{AckedGroupMembership, ForwardSecureOrdering, MessageInfo, MessageType};

    #[derive(Debug)]
    pub struct TestOrderer<DGM> {
        _marker: PhantomData<DGM>,
    }

    impl<DGM> TestOrderer<DGM>
    where
        DGM: Clone + AckedGroupMembership<MemberId, MessageId>,
    {
        pub fn init(my_id: MemberId) -> TestOrdererState<DGM> {
            TestOrdererState {
                next_message_seq: 0,
                last_control_messages: Vec::new(),
                last_application_message: None,
                my_id,
                ready: HashSet::new(),
                ready_queue: VecDeque::new(),
                pending: HashMap::new(),
                messages: HashMap::new(),
            }
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct TestOrdererState<DGM>
    where
        DGM: Clone + AckedGroupMembership<MemberId, MessageId>,
    {
        next_message_seq: usize,
        my_id: MemberId,
        last_application_message: Option<MessageId>,
        last_control_messages: Vec<MessageId>,
        ready: HashSet<MessageId>,
        ready_queue: VecDeque<MessageId>,
        pending: HashMap<MessageId, HashSet<(MessageId, Vec<MessageId>)>>,
        messages: HashMap<MessageId, TestMessage<DGM>>,
    }

    impl<DGM> ForwardSecureOrdering<MemberId, MessageId, DGM> for TestOrderer<DGM>
    where
        DGM: std::fmt::Debug
            + Clone
            + AckedGroupMembership<MemberId, MessageId>
            + Serialize
            + for<'a> Deserialize<'a>,
    {
        type State = TestOrdererState<DGM>;

        type Error = TestOrdererError;

        type Message = TestMessage<DGM>;

        fn next_control_message(
            mut y: Self::State,
            control_message: &ControlMessage<MemberId, MessageId>,
            direct_messages: &[DirectMessage<MemberId, MessageId, DGM>],
        ) -> Result<(Self::State, Self::Message), Self::Error> {
            let seq = y.next_message_seq;
            let sender = y.my_id;
            let previous = y.last_control_messages.clone();

            let message = TestMessage {
                seq,
                sender,
                previous,
                content: TestMessageContent::System {
                    control_message: control_message.to_owned(),
                    direct_messages: direct_messages.to_owned(),
                },
            };

            y.next_message_seq += 1;
            y.last_application_message = None;
            y.last_control_messages = vec![message.id()];

            Ok((y, message))
        }

        fn next_application_message(
            mut y: Self::State,
            generation: Generation,
            ciphertext: Vec<u8>,
        ) -> Result<(Self::State, Self::Message), Self::Error> {
            let seq = y.next_message_seq;
            let sender = y.my_id;

            let previous = if let Some(last_id) = y.last_application_message {
                vec![last_id]
            } else {
                y.last_control_messages.clone()
            };

            let message = TestMessage {
                seq,
                sender,
                previous,
                content: TestMessageContent::Application {
                    ciphertext,
                    generation,
                },
            };

            y.next_message_seq += 1;

            Ok((y, message))
        }

        fn queue(mut y: Self::State, message: &Self::Message) -> Result<Self::State, Self::Error> {
            let id = message.id();

            y.messages.insert(id, message.clone());

            if !Self::ready(&y, &message.previous)? {
                let (y_i, _) = Self::mark_pending(y, id, message.previous.clone())?;
                return Ok(y_i);
            }

            let (y_i, _) = Self::mark_ready(y, id)?;
            let y_ii = Self::process_pending(y_i, id)?;

            Ok(y_ii)
        }

        fn set_welcome(
            y: Self::State,
            message: &Self::Message,
        ) -> Result<Self::State, Self::Error> {
            todo!()
        }

        fn next_ready_message(
            y: Self::State,
        ) -> Result<(Self::State, Option<Self::Message>), Self::Error> {
            let (y_i, next_ready) = Self::take_next_ready(y)?;
            let message = next_ready.map(|id| {
                y_i.messages
                    .get(&id)
                    .expect("ids map consistently to messages")
                    .to_owned()
            });
            Ok((y_i, message))
        }
    }

    impl<DGM> TestOrderer<DGM>
    where
        DGM: Clone + AckedGroupMembership<MemberId, MessageId>,
    {
        fn mark_ready(
            mut y: TestOrdererState<DGM>,
            key: MessageId,
        ) -> Result<(TestOrdererState<DGM>, bool), TestOrdererError> {
            let result = y.ready.insert(key);
            if result {
                y.ready_queue.push_back(key);
            }
            Ok((y, result))
        }

        fn mark_pending(
            mut y: TestOrdererState<DGM>,
            key: MessageId,
            dependencies: Vec<MessageId>,
        ) -> Result<(TestOrdererState<DGM>, bool), TestOrdererError> {
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

        fn get_next_pending(
            y: &TestOrdererState<DGM>,
            key: MessageId,
        ) -> Result<Option<HashSet<(MessageId, Vec<MessageId>)>>, TestOrdererError> {
            Ok(y.pending.get(&key).cloned())
        }

        fn take_next_ready(
            mut y: TestOrdererState<DGM>,
        ) -> Result<(TestOrdererState<DGM>, Option<MessageId>), TestOrdererError> {
            let result = y.ready_queue.pop_front();
            Ok((y, result))
        }

        fn remove_pending(
            mut y: TestOrdererState<DGM>,
            key: MessageId,
        ) -> Result<(TestOrdererState<DGM>, bool), TestOrdererError> {
            let result = y.pending.remove(&key).is_some();
            Ok((y, result))
        }

        fn ready(
            y: &TestOrdererState<DGM>,
            dependencies: &[MessageId],
        ) -> Result<bool, TestOrdererError> {
            let deps_set = HashSet::from_iter(dependencies.iter().cloned());
            let result = y.ready.is_superset(&deps_set);
            Ok(result)
        }

        fn process_pending(
            y: TestOrdererState<DGM>,
            key: MessageId,
        ) -> Result<TestOrdererState<DGM>, TestOrdererError> {
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

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct TestMessage<DGM>
    where
        DGM: Clone + AckedGroupMembership<MemberId, MessageId>,
    {
        seq: usize,
        sender: usize,
        previous: Vec<MessageId>,
        content: TestMessageContent<DGM>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub enum TestMessageContent<DGM>
    where
        DGM: Clone + AckedGroupMembership<MemberId, MessageId>,
    {
        Application {
            ciphertext: Vec<u8>,
            generation: Generation,
        },
        System {
            control_message: ControlMessage<MemberId, MessageId>,
            direct_messages: Vec<DirectMessage<MemberId, MessageId, DGM>>,
        },
    }

    impl<DGM> MessageInfo<MemberId, MessageId, DGM> for TestMessage<DGM>
    where
        DGM: Clone + AckedGroupMembership<MemberId, MessageId>,
    {
        fn id(&self) -> MessageId {
            MessageId {
                sender: self.sender,
                seq: self.seq,
            }
        }

        fn sender(&self) -> MemberId {
            self.sender
        }

        // TODO: Should this be better returning a borrowed type?
        fn message_type(&self) -> MessageType<MemberId, MessageId> {
            match &self.content {
                TestMessageContent::Application {
                    ciphertext,
                    generation,
                } => MessageType::Application {
                    ciphertext: ciphertext.to_owned(),
                    generation: *generation,
                },
                TestMessageContent::System {
                    control_message, ..
                } => MessageType::Control(control_message.to_owned()),
            }
        }

        // TODO: Should this be better returning a borrowed type?
        fn direct_messages(&self) -> Vec<DirectMessage<MemberId, MessageId, DGM>> {
            match &self.content {
                TestMessageContent::Application {
                    ciphertext,
                    generation,
                } => Vec::new(),
                TestMessageContent::System {
                    direct_messages, ..
                } => direct_messages.clone(),
            }
        }
    }

    #[derive(Debug, Error)]
    pub enum TestOrdererError {}
}
