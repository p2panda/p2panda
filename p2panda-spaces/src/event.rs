// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::types::ActorId;

pub enum Event {
    Application { space_id: ActorId, data: Vec<u8> },
}
