// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::hash::Hash as StdHash;

use crate::address_book::NodeInfo;
use crate::address_book::memory::MemoryStore;
use crate::traits::SubscriptionInfo;

pub type TestId = usize;

pub type TestTopic = String;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, StdHash)]
pub struct TestInfo {
    pub id: TestId,
    pub bootstrap: bool,
    pub timestamp: u64,
}

impl TestInfo {
    pub fn new(id: TestId) -> Self {
        Self {
            id,
            bootstrap: false,
            timestamp: 0,
        }
    }

    pub fn new_bootstrap(id: TestId) -> Self {
        Self {
            id,
            bootstrap: true,
            timestamp: 0,
        }
    }
}

impl NodeInfo<TestId> for TestInfo {
    fn id(&self) -> TestId {
        self.id
    }

    fn is_bootstrap(&self) -> bool {
        self.bootstrap
    }

    fn timestamp(&self) -> u64 {
        self.timestamp
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
