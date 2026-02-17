// SPDX-License-Identifier: MIT OR Apache-2.0

use rand::RngExt;
use rand_chacha::ChaCha20Rng;

use crate::address_book::memory::current_timestamp;
use crate::address_book::traits::NodeInfo;

pub type TestNodeId = usize;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TestNodeInfo {
    pub id: TestNodeId,
    pub bootstrap: bool,
    pub transports: Option<TestTransportInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TestTransportInfo {
    address: String,
    timestamp: u64,
}

impl TestTransportInfo {
    pub fn new(address: &str) -> Self {
        Self {
            address: address.to_owned(),
            timestamp: current_timestamp(),
        }
    }
}

impl TestNodeInfo {
    pub fn new(id: TestNodeId) -> Self {
        Self {
            id,
            bootstrap: false,
            transports: None,
        }
    }

    pub fn new_bootstrap(id: TestNodeId) -> Self {
        Self {
            id,
            bootstrap: true,
            transports: None,
        }
    }

    pub fn with_random_address(mut self, rng: &mut ChaCha20Rng) -> Self {
        self.transports = Some(TestTransportInfo {
            timestamp: current_timestamp(),
            address: {
                // Generate a random, fake IPv4 address
                let segments: [u8; 4] = rng.random();
                segments.map(|s| s.to_string()).join(".")
            },
        });
        self
    }

    /// Returns true if the given transport information is newer than what we have already and it
    /// got updated.
    pub fn update_transports(&mut self, transports: TestTransportInfo) -> bool {
        match self.transports {
            Some(ref current) => {
                if current.timestamp < transports.timestamp {
                    self.transports = Some(transports);
                    return true;
                }
            }
            None => {
                self.transports = Some(transports);
                return true;
            }
        }
        false
    }
}

impl NodeInfo<TestNodeId> for TestNodeInfo {
    type Transports = TestTransportInfo;

    fn id(&self) -> TestNodeId {
        self.id
    }

    fn is_bootstrap(&self) -> bool {
        self.bootstrap
    }

    fn is_stale(&self) -> bool {
        false
    }

    fn transports(&self) -> Option<Self::Transports> {
        self.transports.clone()
    }
}
