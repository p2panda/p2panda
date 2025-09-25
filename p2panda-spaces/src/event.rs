// SPDX-License-Identifier: MIT OR Apache-2.0

pub enum Event<ID> {
    Application { space_id: ID, data: Vec<u8> },
    Removed { space_id: ID },
}
