// SPDX-License-Identifier: MIT OR Apache-2.0

// @TODO: Move all of this into new `p2panda-store` when ready.

pub trait AddressBookStore {}

#[derive(Clone)]
pub struct MemoryStore {}

impl MemoryStore {
    pub fn new() -> Self {
        Self {}
    }
}

impl AddressBookStore for MemoryStore {}
