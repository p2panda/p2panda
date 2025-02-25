// SPDX-License-Identifier: MIT OR Apache-2.0

use async_stream::stream;
use futures_util::Stream;
use p2panda_core::prune::PruneFlag;
use p2panda_core::{Body, Extension, Hash, Header, PrivateKey, PublicKey, RawOperation};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct StreamName(PublicKey, Option<String>);

impl StreamName {
    pub fn new(public_key: PublicKey, name: Option<&str>) -> Self {
        Self(public_key, name.map(|value| value.to_owned()))
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Extensions {
    #[serde(rename = "s")]
    pub stream_name: StreamName,

    #[serde(
        rename = "p",
        skip_serializing_if = "PruneFlag::is_not_set",
        default = "PruneFlag::default"
    )]
    pub prune_flag: PruneFlag,
}

impl Extension<StreamName> for Extensions {
    fn extract(header: &Header<Self>) -> Option<StreamName> {
        header
            .extensions
            .as_ref()
            .map(|extensions| extensions.stream_name.clone())
    }
}

impl Extension<PruneFlag> for Extensions {
    fn extract(header: &Header<Self>) -> Option<PruneFlag> {
        header
            .extensions
            .as_ref()
            .map(|extensions| extensions.prune_flag.clone())
    }
}

pub struct Log {
    private_key: PrivateKey,
    next_backlink: Option<Hash>,
    next_seq_num: u64,
}

impl Log {
    pub fn new() -> Self {
        Self {
            private_key: PrivateKey::new(),
            next_backlink: None,
            next_seq_num: 0,
        }
    }

    pub fn create_operation(&mut self) -> (Header<Extensions>, Option<Body>) {
        let public_key = self.private_key.public_key();

        let extensions = Extensions {
            stream_name: StreamName::new(public_key, Some("chat")),
            ..Default::default()
        };

        let body = Body::new(b"Blub, Jellyfish!");

        let mut header = Header::<Extensions> {
            public_key,
            version: 1,
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp: 0,
            seq_num: self.next_seq_num,
            backlink: self.next_backlink,
            previous: vec![],
            extensions: Some(extensions),
        };
        header.sign(&self.private_key);

        self.next_backlink = Some(header.hash());
        self.next_seq_num += 1;

        (header, Some(body))
    }
}

pub fn mock_stream() -> impl Stream<Item = RawOperation> {
    let mut log = Log::new();
    stream! {
        loop {
            let (header, body) = log.create_operation();
            yield (header.to_bytes(), body.map(|body| body.to_bytes()));
        }
    }
}

pub fn generate_operations(num: usize) -> Vec<(Header<Extensions>, Option<Body>)> {
    let mut operations = Vec::with_capacity(num);
    let mut log = Log::new();
    for _ in 0..num {
        operations.push(log.create_operation());
    }
    operations
}
