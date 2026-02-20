// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, HashSet};
use std::convert::Infallible;

use p2panda_store::address_book::test_utils::{TestNodeId, TestNodeInfo, TestTransportInfo};

use crate::DiscoveryResult;
use crate::traits::LocalTopics;

#[derive(Clone, Default, Debug)]
pub struct TestSubscription {
    pub topics: HashSet<[u8; 32]>,
}

impl LocalTopics for TestSubscription {
    type Error = Infallible;

    async fn topics(&self) -> Result<HashSet<[u8; 32]>, Self::Error> {
        Ok(self.topics.clone())
    }
}

impl DiscoveryResult<TestNodeId, TestNodeInfo> {
    pub fn from_neighbors(remote_node_id: TestNodeId, node_ids: &[TestNodeId]) -> Self {
        Self {
            remote_node_id,
            transport_infos: BTreeMap::from_iter(
                node_ids
                    .iter()
                    .map(|id| (*id, TestTransportInfo::new("test"))),
            ),
            topics: HashSet::new(),
        }
    }
}
