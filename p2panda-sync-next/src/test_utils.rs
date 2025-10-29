// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{collections::HashMap, convert::Infallible};

use p2panda_core::{Body, Hash, Header, PrivateKey};
use serde::{Deserialize, Serialize};

use crate::log_sync::Logs;
use crate::topic_log_sync::TopicLogMap;
use crate::traits::TopicQuery;

pub fn create_operation(
    private_key: &PrivateKey,
    body: &Body,
    seq_num: u64,
    timestamp: u64,
    backlink: Option<Hash>,
) -> (Header, Vec<u8>) {
    let mut header = Header {
        version: 1,
        public_key: private_key.public_key(),
        signature: None,
        payload_size: body.size(),
        payload_hash: Some(body.hash()),
        timestamp,
        seq_num,
        backlink,
        previous: vec![],
        extensions: Some(()),
    };
    header.sign(private_key);
    let header_bytes = header.to_bytes();
    (header, header_bytes)
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct LogHeightTopic(String);

impl LogHeightTopic {
    pub fn new(name: &str) -> Self {
        Self(name.to_owned())
    }
}

impl TopicQuery for LogHeightTopic {}

#[derive(Clone, Debug)]
pub struct LogHeightTopicMap<T>(HashMap<T, Logs<u64>>);

impl<T> LogHeightTopicMap<T>
where
    T: TopicQuery,
{
    pub fn new() -> Self {
        LogHeightTopicMap(HashMap::new())
    }

    pub fn insert(&mut self, topic_query: &T, logs: Logs<u64>) -> Option<Logs<u64>> {
        self.0.insert(topic_query.clone(), logs)
    }
}

impl<T> TopicLogMap<T, u64> for LogHeightTopicMap<T>
where
    T: TopicQuery,
{
    type Error = Infallible;

    async fn get(&self, topic_query: &T) -> Result<Logs<u64>, Self::Error> {
        Ok(self.0.get(topic_query).cloned().unwrap_or_default())
    }
}
