// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::xchacha20::XAeadNonce;
use crate::data_scheme::{ControlMessage, DirectMessage, GroupSecretId};
use crate::ordering::{Orderer, OrdererError, OrdererState};
use crate::test_utils::{MemberId, MessageId};
use crate::traits::{GroupMembership, GroupMessage, GroupMessageContent, Ordering};

/// Orderer for testing the "data encryption" group APIs.
///
/// This is sufficient for the current testing setup but for anything "production ready" a more
/// sophisticated solution will be required as all messages are kept in memory.
#[derive(Clone, Debug)]
pub struct MessageOrderer<DGM> {
    _marker: PhantomData<DGM>,
}

impl<DGM> MessageOrderer<DGM>
where
    DGM: Clone + GroupMembership<MemberId, MessageId>,
{
    pub fn init(my_id: MemberId) -> MessageOrdererState<DGM> {
        MessageOrdererState {
            next_message_seq: 0,
            previous: HashMap::new(),
            orderer: Orderer::init(),
            my_id,
            messages: HashMap::new(),
            welcome_message: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageOrdererState<DGM>
where
    DGM: Clone + GroupMembership<MemberId, MessageId>,
{
    /// Sequence number of the next, message to-be published.
    next_message_seq: usize,

    /// Our own member id.
    my_id: MemberId,

    /// Internal helper to order messages based on their "previous" dependencies.
    orderer: OrdererState<MessageId>,

    /// Latest known message id's from each group member. This is the "head" of the DAG.
    previous: HashMap<MemberId, MessageId>,

    /// In-memory store of all messages.
    messages: HashMap<MessageId, TestMessage<DGM>>,

    /// "Create" or "Add" message which got us into the group.
    welcome_message: Option<TestMessage<DGM>>,
}

impl<DGM> Ordering<MemberId, MessageId, DGM> for MessageOrderer<DGM>
where
    DGM: std::fmt::Debug
        + Clone
        + GroupMembership<MemberId, MessageId>
        + Serialize
        + for<'a> Deserialize<'a>,
{
    type State = MessageOrdererState<DGM>;

    type Error = MessageOrdererError;

    type Message = TestMessage<DGM>;

    fn next_control_message(
        mut y: Self::State,
        control_message: &ControlMessage<MemberId>,
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
        group_secret_id: GroupSecretId,
        nonce: XAeadNonce,
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
                group_secret_id,
                nonce,
            },
        };

        y.next_message_seq += 1;

        Ok((y, message))
    }

    fn queue(mut y: Self::State, message: &Self::Message) -> Result<Self::State, Self::Error> {
        let id = message.id();

        // TODO: We keep all messages in memory currently which is bad. This needs a persistence
        // layer as soon as we've looked into how it all plays together with our access control and
        // stream APIs.
        y.messages.insert(id, message.clone());

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
        mut y: Self::State,
    ) -> Result<(Self::State, Option<Self::Message>), Self::Error> {
        // We have not joined the group yet, don't process any messages yet.
        if y.welcome_message.is_none() {
            return Ok((y, None));
        };

        let (y_orderer_i, next_ready) = Orderer::take_next_ready(y.orderer)?;
        y.orderer = y_orderer_i;

        let message = next_ready.map(|id| {
            y.messages
                .get(&id)
                .expect("ids map consistently to messages")
                .to_owned()
        });

        if let Some(ref message) = message {
            if let GroupMessageContent::Control(_) = message.content() {
                // Mark messages as "last seen" so we can mention the "previous" ones as soon
                // as we publish a message ourselves.
                y.previous.insert(message.sender(), message.id());
            }
        }

        Ok((y, message))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestMessage<DGM>
where
    DGM: Clone + GroupMembership<MemberId, MessageId>,
{
    seq: usize,
    sender: usize,
    previous: Vec<MessageId>,
    content: TestMessageContent<DGM>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TestMessageContent<DGM>
where
    DGM: Clone + GroupMembership<MemberId, MessageId>,
{
    Application {
        ciphertext: Vec<u8>,
        group_secret_id: GroupSecretId,
        nonce: XAeadNonce,
    },
    System {
        control_message: ControlMessage<MemberId>,
        direct_messages: Vec<DirectMessage<MemberId, MessageId, DGM>>,
    },
}

impl<DGM> GroupMessage<MemberId, MessageId, DGM> for TestMessage<DGM>
where
    DGM: Clone + GroupMembership<MemberId, MessageId>,
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

    fn content(&self) -> GroupMessageContent<MemberId> {
        match &self.content {
            TestMessageContent::Application {
                ciphertext,
                group_secret_id,
                nonce,
            } => GroupMessageContent::Application {
                group_secret_id: *group_secret_id,
                nonce: *nonce,
                ciphertext: ciphertext.to_vec(),
            },
            TestMessageContent::System {
                control_message, ..
            } => GroupMessageContent::Control(control_message.clone()),
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
pub enum MessageOrdererError {
    #[error(transparent)]
    Orderer(#[from] OrdererError),
}
