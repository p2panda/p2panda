// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::message_scheme::{ControlMessage, DirectMessage};
use crate::traits::{AckedGroupMembership, MessageInfo};

/// Peers need to make sure that messages arrive "in order" to be processed correctly. For
/// p2panda's "message encryption" scheme extra care is required, since the strong forward-secrecy
/// guarantees makes ordering more strict.
///
/// We require three things:
///
/// 1. Define a way to partially order our messages (for example through a vector clock), like this
///    we can sort events "after" or "before" each other, or identify messages which arrived "at
///    the same time".
/// 2. Define a way to declare "dependencies", that is, messages which are required to be processed
///    _before_ we can process this message. This is slightly different from a vector clock as we
///    do not only declare which message we've observed "before" to help with partial ordering, but
///    also point at additional requirements to fullfil the protocol.
/// 3. Define a set of rules, the "protocol", peers need to follow whenever they publish new
///    messages: What information do they need to mention for other peers to correctly order and
///    process messages from us?
///
/// When publishing a message peers need to make sure to provide the following informations:
///
/// 1. "create" control messages do not have any dependencies as they are the first messages in a
///    group.
/// 2. When an "add", "update" or "remove" control message gets published, that message needs to
///    point at a) the last known, previously processed control messages (by us and others), b) if
///    any application messages were sent by us, the last sent message. The latter helps with peers
///    understanding that they might miss a message when they switch to a new ratchet, they can
///    decide to ignore this message "dependency", but will also then potentially loose it. This
///    can be useful to do if messages get lost and peers otherwise get "stuck".
/// 3. "ack" control messages need to point at the regarding "create", "add", "update" or "remove"
///    control message they are acknowledging.
/// 4. The first application message written during a new "ratchet epoch" needs to point at the
///    "ack" or "create", "add", "update" or "remove" message which initiated that epoch.
/// 5. Every sub-sequent application message needs to point at the previous application message.
///
/// In this example a user "Alice" creates a group with Bob. Both of them send messages into the
/// group ("Message 1", "Messsage 2" etc.) based on the established ratchet secrets. At some point
/// Alice decides to renew the group's seed with an "update", and at the same time (concurrently)
/// Bob "adds" Charlie. After processing all messages in the correct order and meeting all
/// dependencies Alice and Bob will be able to read all sent messages by each other.
///
/// ```text
///
///        Alice
///       ────────
///       ┌──────┐
///       │CREATE│
///       └──────┘                   Bob
///         ▲ ▲ ▲                   ─────
///         │ │ │                   ┌───┐
///         │ │ └───────────────────┤ACK│
///         │ │                     └───┘
///         │ │                      ▲ ▲
///           │                      │ │
/// Message 1 │                      │
///         ▲ │                      │ Message 1
///         │ │                      │ ▲
///           │                      │ │
/// Message 2 │                      │
///         ▲ │                      │ Message 2
///         │ │                      │ ▲
///         │ │                      │ │
///        ┌┴─┴───┐                 ┌┴─┴┐
///        │UPDATE│   Concurrent!   │ADD│
///        └──────┘                 └───┘
///         ▲    ▲                   ▲ ▲                   Charlie
///         │    │                   │ │                  ─────────
///         │ ┌──┼───────────────────┘ ├──────────────────────┐
///           │  │                     │                      │
/// Message 3 │  └───────────────────┐ │                      │
///         ▲ │                      │                        │
///         │ │                      │ Message 3              │
///           │                      │ ▲                      │
/// Message 4 │                      │ │                      │
///           │                      │ │                      │
///         ┌─┴─┐                   ┌┴─┴┐                   ┌─┴─┐
///         │ACK│                   │ACK│                   │ACK│
///         └───┘                   └───┘                   └───┘
/// ```
///
/// When a peer processes a "welcome" message (they got added to a group, like "Charlie" in our
/// example), then the following steps take place:
///
/// 1. All messages prior to the "welcome" message (the "add" which added us) can be ignored.
/// 2. All messages after or concurrent to the "welcome" message need to be processed regularily
///    like all other messages.
/// 3. All application messages concurrent to the "welcome" message can be ignored (as they can not
///    be decrypted).
///
/// All of this "welcome" processing needs to be done before we can move on processing future
/// messages.
///
/// In the previously given example "Charlie" would be added to the group by Bob's "add" control
/// message. Charlie would process their "welcome", acknowledge it and look at all other messages
/// now. They identified that Alice's "update" happened concurrently to the "add", so they also
/// process this message. They ignore the "create" as it took place before the "add". They ignore
/// "Message 1", "Message 2", "Message 3" and "Message 4" of Alice and "Message 1" and "Message 2"
/// of Bob, as they would not be able to decrypt them. Afterwards they would be able to decrypt
/// "Message 3" of Bob as this message was created with Charlie in mind.
///
/// Note that Charlie will _not_ be able to decrypt "Message 3" and "Message 4" of Alice as they
/// have been encrypted by Alice prior to their knowledge that Charlie was already in the group
/// then. As soon as Alice will learn that Charlie was added they will "forward" their ratchet
/// state to Charlie, but this will only be used for future messages.
pub trait ForwardSecureOrdering<ID, OP, DGM>
where
    DGM: AckedGroupMembership,
{
    type State: Clone + Debug + Serialize + for<'a> Deserialize<'a>;

    type Error: Error;

    type Message: MessageInfo<ID, OP>;

    fn next_control_message(
        y: Self::State,
        control_message: &ControlMessage<ID, OP>,
        direct_messages: &[DirectMessage<ID, OP, DGM>],
    ) -> Result<(Self::State, Self::Message), Self::Error>;
}
