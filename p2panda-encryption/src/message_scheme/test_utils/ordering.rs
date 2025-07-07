// SPDX-License-Identifier: MIT OR Apache-2.0

// TODO: A complete ordering solution following the full ordering specification for the "message
// encryption" scheme will be provided as soon as "access control" work has been finished.
use std::collections::HashMap;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::message_scheme::{ControlMessage, DirectMessage, Generation};
use crate::ordering::{Orderer, OrdererError, OrdererState};
use crate::test_utils::{MemberId, MessageId};
use crate::traits::{
    AckedGroupMembership, ForwardSecureGroupMessage, ForwardSecureMessageContent,
    ForwardSecureOrdering,
};

/// Simplified orderer for testing the "message encryption" group APIs.
///
/// NOTE: This orderer does _not_ fullfill the full specification for correct ordering. It's
/// assuming that peers process all messages after each member has published max. one control
/// or application message. On top it's very inefficient, as every published message points at
/// _every_ previously published messages from all peers.
///
/// This is sufficient for the current testing setup but for anything "production ready" and
/// more robust for all concurrency scenarios, a more sophisticated solution will be required.
#[derive(Clone, Debug)]
pub struct ForwardSecureOrderer<DGM> {
    _marker: PhantomData<DGM>,
}

impl<DGM> ForwardSecureOrderer<DGM>
where
    DGM: Clone + AckedGroupMembership<MemberId, MessageId>,
{
    pub fn init(my_id: MemberId) -> ForwardSecureOrdererState<DGM> {
        ForwardSecureOrdererState {
            next_message_seq: 0,
            orderer: Orderer::init(),
            my_id,
            messages: HashMap::new(),
            welcome_message: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForwardSecureOrdererState<DGM>
where
    DGM: Clone + AckedGroupMembership<MemberId, MessageId>,
{
    /// Sequence number of the next, message to-be published.
    next_message_seq: usize,

    /// Our own member id.
    my_id: MemberId,

    /// Internal helper to order messages based on their "previous" dependencies.
    orderer: OrdererState<MessageId>,

    /// In-memory store of all messages.
    messages: HashMap<MessageId, TestMessage<DGM>>,

    /// "Create" or "Add" message which got us into the group.
    welcome_message: Option<TestMessage<DGM>>,
}

impl<DGM> ForwardSecureOrdering<MemberId, MessageId, DGM> for ForwardSecureOrderer<DGM>
where
    DGM: std::fmt::Debug
        + Clone
        + AckedGroupMembership<MemberId, MessageId>
        + Serialize
        + for<'a> Deserialize<'a>,
{
    type State = ForwardSecureOrdererState<DGM>;

    type Error = ForwardSecureOrdererError;

    type Message = TestMessage<DGM>;

    fn next_control_message(
        mut y: Self::State,
        control_message: &ControlMessage<MemberId, MessageId>,
        direct_messages: &[DirectMessage<MemberId, MessageId, DGM>],
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        let seq = y.next_message_seq;
        let sender = y.my_id;

        // This is a very naive implementation where every message points at _every_ known,
        // previous message as an "dependency". This allows us to not write any code which tracks
        // transitive dependencies.
        let previous = y.messages.keys().cloned().collect();

        let message = TestMessage {
            seq,
            sender,
            previous,
            content: TestMessageContent::System {
                control_message: control_message.to_owned(),
                direct_messages: direct_messages.to_owned(),
            },
        };

        y.messages.insert(message.id(), message.clone());
        y.next_message_seq += 1;

        Ok((y, message))
    }

    fn next_application_message(
        mut y: Self::State,
        generation: Generation,
        ciphertext: Vec<u8>,
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        let seq = y.next_message_seq;
        let sender = y.my_id;

        // This is a very naive implementation where every message points at _every_ known,
        // previous message as an "dependency". This allows us to not write any code which tracks
        // transitive dependencies.
        let previous = y.messages.keys().cloned().collect();

        let message = TestMessage {
            seq,
            sender,
            previous,
            content: TestMessageContent::Application {
                ciphertext,
                generation,
            },
        };

        y.messages.insert(message.id(), message.clone());
        y.next_message_seq += 1;

        Ok((y, message))
    }

    fn queue(mut y: Self::State, message: &Self::Message) -> Result<Self::State, Self::Error> {
        let id = message.id();

        y.messages.insert(id, message.clone());

        // Clear dependencies list from own messages, we didn't queue them as we know that we've
        // seen and processed them.
        let previous: Vec<MessageId> = message
            .previous
            .iter()
            .filter(|id| id.sender != y.my_id)
            .cloned()
            .collect();

        if !Orderer::ready(&y.orderer, &previous)? {
            let (y_orderer_i, _) = Orderer::mark_pending(y.orderer, id, previous)?;
            y.orderer = y_orderer_i;
            return Ok(y);
        }

        let (y_orderer_i, _) = Orderer::mark_ready(y.orderer, id)?;
        let y_orderer_ii = Orderer::process_pending(y_orderer_i, id)?;
        y.orderer = y_orderer_ii;

        Ok(y)
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
            let (y_next, next_ready) = Orderer::take_next_ready(y_loop.orderer)?;
            y_loop.orderer = y_next;

            let message = next_ready.map(|id| {
                y_loop
                    .messages
                    .get(&id)
                    .expect("ids map consistently to messages")
                    .to_owned()
            });

            if let Some(message) = message {
                // Don't forward welcome message, it was already processed.
                if message.id() == welcome.id() {
                    continue;
                }

                // Control messages can be ignored if message is before our welcome. Concurrent
                // messages need to be processed.
                //
                // This is a naive implementation where we assume that every member processed every
                // control message after one round and where every message points at _every_
                // previously-created message.
                if let ForwardSecureMessageContent::Control { .. } = message.content() {
                    if welcome.previous.contains(&message.id()) {
                        continue;
                    }
                }

                // Application messages can be ignored if before or concurrent to welcome.
                if let ForwardSecureMessageContent::Application { .. } = message.content() {
                    if !message.previous.contains(&welcome.id()) {
                        continue;
                    }
                }

                return Ok((y_loop, Some(message)));
            } else {
                return Ok((y_loop, None));
            }
        }
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

impl<DGM> ForwardSecureGroupMessage<MemberId, MessageId, DGM> for TestMessage<DGM>
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

    fn content(&self) -> ForwardSecureMessageContent<MemberId, MessageId> {
        match &self.content {
            TestMessageContent::Application {
                ciphertext,
                generation,
            } => ForwardSecureMessageContent::Application {
                ciphertext: ciphertext.to_owned(),
                generation: *generation,
            },
            TestMessageContent::System {
                control_message, ..
            } => ForwardSecureMessageContent::Control(control_message.to_owned()),
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
pub enum ForwardSecureOrdererError {
    #[error(transparent)]
    Orderer(#[from] OrdererError),
}
