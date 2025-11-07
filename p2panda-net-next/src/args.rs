// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::PrivateKey;

#[derive(Clone, Debug)]
pub struct ApplicationArguments<S> {
    pub private_key: PrivateKey,
    pub store: S,
}

#[cfg(test)]
impl<S> ApplicationArguments<S> {
    fn from_store(store: S) -> Self {
        Self {
            private_key: Default::default(),
            store,
        }
    }
}
