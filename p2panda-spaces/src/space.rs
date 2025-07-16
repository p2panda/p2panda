// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::manager::Manager;

/// Encrypted data context with authorization boundary.
///
/// Only members with suitable access to the space can read and write to it.
pub struct Space<S, F, M> {
    manager: Manager<S, F, M>,
}

impl<S, F, M> Space<S, F, M> {
    pub fn publish(_bytes: &[u8]) {
        todo!()
    }

    pub fn process(&mut self, _message: &M) {
        todo!()
    }
}
