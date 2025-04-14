// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Display;

use crate::message_scheme::{ControlMessage, DirectMessage, Generation};
use crate::traits::AckedGroupMembership;

// TODO: Find better names and distinction with "data scheme" messages.
pub trait MessageInfo<ID, OP, DGM>
where
    DGM: AckedGroupMembership<ID, OP>,
{
    fn id(&self) -> OP;

    fn sender(&self) -> ID;

    fn message_type(&self) -> MessageType<ID, OP>;

    fn direct_messages(&self) -> Vec<DirectMessage<ID, OP, DGM>>;
}

#[derive(Debug)]
pub enum MessageType<ID, OP> {
    Control(ControlMessage<ID, OP>),
    Application {
        ciphertext: Vec<u8>,
        generation: Generation,
    },
}

impl<ID, OP> Display for MessageType<ID, OP> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                MessageType::Control(control_message) => control_message.to_string(),
                MessageType::Application { generation, .. } =>
                    format!("application @{}", generation),
            }
        )
    }
}
