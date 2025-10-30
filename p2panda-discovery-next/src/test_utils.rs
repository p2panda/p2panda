// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, HashSet};
use std::convert::Infallible;
use std::hash::Hash as StdHash;

use rand::Rng;
use rand_chacha::ChaCha20Rng;

use crate::DiscoveryResult;
use crate::address_book::NodeInfo;
use crate::address_book::memory::{MemoryStore, current_timestamp};
use crate::traits::SubscriptionInfo;

pub type TestId = usize;

pub type TestTopic = String;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, StdHash)]
pub struct TestInfo {
    pub id: TestId,
    pub bootstrap: bool,
    pub transports: Option<TestTransportInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, StdHash)]
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

impl TestInfo {
    pub fn new(id: TestId) -> Self {
        Self {
            id,
            bootstrap: false,
            transports: None,
        }
    }

    pub fn new_bootstrap(id: TestId) -> Self {
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

impl NodeInfo<TestId> for TestInfo {
    type Transports = TestTransportInfo;

    fn id(&self) -> TestId {
        self.id
    }

    fn is_bootstrap(&self) -> bool {
        self.bootstrap
    }

    fn transports(&self) -> Option<Self::Transports> {
        self.transports.clone()
    }
}

pub type TestStore<R> = MemoryStore<R, TestTopic, TestId, TestInfo>;

#[derive(Clone, Default, Debug)]
pub struct TestSubscription {
    pub topics: Vec<TestTopic>,
    pub topic_ids: Vec<[u8; 32]>,
}

impl SubscriptionInfo<TestTopic> for TestSubscription {
    type Error = Infallible;

    async fn subscribed_topics(&self) -> Result<Vec<TestTopic>, Self::Error> {
        Ok(self.topics.clone())
    }

    async fn subscribed_topic_ids(&self) -> Result<Vec<[u8; 32]>, Self::Error> {
        Ok(self.topic_ids.clone())
    }
}

impl DiscoveryResult<TestTopic, TestId, TestInfo> {
    pub fn from_neighbors(remote_node_id: TestId, node_ids: &[TestId]) -> Self {
        Self {
            remote_node_id,
            node_transport_infos: BTreeMap::from_iter(
                node_ids
                    .iter()
                    .map(|id| (id.clone(), TestTransportInfo::new("test"))),
            ),
            node_topics: HashSet::new(),
            node_topic_ids: HashSet::new(),
        }
    }
}
