// SPDX-License-Identifier: MIT OR Apache-2.0

use iroh::protocol::DynProtocolHandler as ProtocolHandler;

use crate::protocols::ProtocolMap;

#[derive(Debug)]
#[allow(dead_code)]
pub struct NetworkBuilder {
    protocols: ProtocolMap,
}

impl NetworkBuilder {
    /// Adds a custom protocol for communication between two peers.
    pub fn _protocol(mut self, identifier: Vec<u8>, handler: impl ProtocolHandler) -> Self {
        self.protocols.insert(identifier, Box::new(handler));
        self
    }
}
