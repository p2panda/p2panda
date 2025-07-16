// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::manager::Manager;

/// Encrypted data context with authorization boundary.
///
/// Only members with suitable access to the space can read and write to it.
pub struct Space<S, F, M> {
    manager: Manager<S, F, M>,
}

impl<S, F, M> Space<S, F, M> {
    pub(crate) fn create(manager: Manager<S, F, M>) -> Self {

        // 1. derive a space id
        //    - generate new key pair
        //    - use public key for space id
        //    - use the private key to sign the control message
        //    - throw away the private key
        // 2. establish auth group state with create control message
        // 3. establish encryption group state with create control message
        // 4. merge and sign control messages in forge (F)
        // 5. persist new state

        Self { manager }
    }

    pub fn publish(_bytes: &[u8]) {
        todo!()
    }

    pub fn process(&mut self, _message: &M) {
        todo!()
    }
}
