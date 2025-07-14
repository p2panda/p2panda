// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::manager::Manager;

/// Encrypted data context with authorization boundary.
///
/// Only members with suitable access to the space can read and write to it.
pub struct Space<F, M> {
    manager: Manager<F, M>,
}

impl<F, M> Space<F, M> {
    pub fn publish(_bytes: &[u8]) {
        todo!()
    }
}
