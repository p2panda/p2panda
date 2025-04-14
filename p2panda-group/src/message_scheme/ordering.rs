// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils {
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::marker::PhantomData;

    use serde::{Deserialize, Serialize};
    use thiserror::Error;

    use crate::message_scheme::test_utils::{MemberId, MessageId};
    use crate::message_scheme::{ControlMessage, DirectMessage, Generation};
    use crate::traits::{
        AckedGroupMembership, ForwardSecureMessage, ForwardSecureMessageType, ForwardSecureOrdering,
    };

    /// Simplified orderer for tests.
    ///
    /// This orderer does _not_ fullfill the full specification for correct ordering. It's assuming
    /// that peers process all messages after each member has published max. one control or
    /// application message.
    ///
    /// This is sufficient for the current testing setup but for anything "production ready" and
    /// more robust for all concurrency scenarios, a more sophisticated solution will be required.
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
                previous: HashMap::new(),
                my_id,
                ready: HashSet::new(),
                ready_queue: VecDeque::new(),
                pending: HashMap::new(),
                messages: HashMap::new(),
                welcome_message: None,
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
        previous: HashMap<MemberId, MessageId>,
        ready: HashSet<MessageId>,
        ready_queue: VecDeque<MessageId>,
        pending: HashMap<MessageId, HashSet<(MessageId, Vec<MessageId>)>>,
        messages: HashMap<MessageId, TestMessage<DGM>>,
        welcome_message: Option<TestMessage<DGM>>,
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
            let previous = y.previous.values().cloned().collect();

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
            y.previous.insert(y.my_id, message.id());

            Ok((y, message))
        }

        fn next_application_message(
            mut y: Self::State,
            generation: Generation,
            ciphertext: Vec<u8>,
        ) -> Result<(Self::State, Self::Message), Self::Error> {
            let seq = y.next_message_seq;
            let sender = y.my_id;
            let previous = y.previous.values().cloned().collect();

            let message = TestMessage {
                seq,
                sender,
                previous,
                content: TestMessageContent::Application {
                    ciphertext,
                    generation,
                },
            };

            y.previous.insert(y.my_id, message.id());
            y.next_message_seq += 1;

            Ok((y, message))
        }

        fn queue(mut y: Self::State, message: &Self::Message) -> Result<Self::State, Self::Error> {
            let id = message.id();

            y.messages.insert(id, message.clone());

            let previous: Vec<MessageId> = message
                .previous
                .iter()
                .filter(|id| id.sender != y.my_id)
                .cloned()
                .collect();

            if !Self::ready(&y, &previous)? {
                let (y_i, _) = Self::mark_pending(y, id, previous)?;
                return Ok(y_i);
            }

            let (y_i, _) = Self::mark_ready(y, id)?;
            let y_ii = Self::process_pending(y_i, id)?;

            Ok(y_ii)
        }

        fn set_welcome(
            mut y: Self::State,
            message: &Self::Message,
        ) -> Result<Self::State, Self::Error> {
            y.welcome_message = Some(message.clone());
            Ok(y)
        }

        fn next_ready_message(
            y: Self::State,
        ) -> Result<(Self::State, Option<Self::Message>), Self::Error> {
            // We have not joined the group yet, don't process any messages yet.
            let Some(welcome) = y.welcome_message.clone() else {
                return Ok((y, None));
            };

            let mut y_loop = y;
            loop {
                let (y_next, next_ready) = Self::take_next_ready(y_loop)?;
                y_loop = y_next;

                let message = next_ready.map(|id| {
                    y_loop
                        .messages
                        .get(&id)
                        .expect("ids map consistently to messages")
                        .to_owned()
                });

                if let Some(message) = message {
                    let last_seq = welcome
                        .previous
                        .iter()
                        .find(|msg| msg.sender == message.sender())
                        .map(|msg| msg.seq);

                    // Is this message before our welcome?
                    if let Some(last_seq) = last_seq {
                        if message.id().seq < last_seq + 1 {
                            continue;
                        }
                    }

                    // Mark messages as "last seen" so we can mention the "previous" ones as soon
                    // as we publish a message ourselves.
                    //
                    // In a correct implementation we would _only_ track control messages here (and
                    // not also application messages).
                    y_loop.previous.insert(message.sender(), message.id());

                    return Ok((y_loop, Some(message)));
                } else {
                    return Ok((y_loop, None));
                }
            }
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

        #[allow(clippy::type_complexity)]
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

    impl<DGM> ForwardSecureMessage<MemberId, MessageId, DGM> for TestMessage<DGM>
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

        fn message_type(&self) -> ForwardSecureMessageType<MemberId, MessageId> {
            match &self.content {
                TestMessageContent::Application {
                    ciphertext,
                    generation,
                } => ForwardSecureMessageType::Application {
                    ciphertext: ciphertext.to_owned(),
                    generation: *generation,
                },
                TestMessageContent::System {
                    control_message, ..
                } => ForwardSecureMessageType::Control(control_message.to_owned()),
            }
        }

        fn direct_messages(&self) -> Vec<DirectMessage<MemberId, MessageId, DGM>> {
            match &self.content {
                TestMessageContent::Application { .. } => Vec::new(),
                TestMessageContent::System {
                    direct_messages, ..
                } => direct_messages.clone(),
            }
        }
    }

    #[derive(Debug, Error)]
    pub enum TestOrdererError {}
}
