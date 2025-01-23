// SPDX-License-Identifier: MIT OR Apache-2.0

use async_stream::stream;
use futures_util::Stream;
use p2panda_core::prune::PruneFlag;
use p2panda_core::{Body, Extension, Header, PrivateKey, PublicKey, RawOperation};
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

pub fn mock_stream() -> impl Stream<Item = RawOperation> {
    let private_key = PrivateKey::new();
    let public_key = private_key.public_key();
    let body = Body::new(b"Hello, Penguin!");

    let mut backlink = None;
    let mut seq_num = 0;

    stream! {
        loop {
            let extensions = Extensions {
                stream_name: StreamName::new(public_key, Some("chat")),
                ..Default::default()
            };

            let mut header = Header::<Extensions> {
                public_key,
                version: 1,
                signature: None,
                payload_size: body.size(),
                payload_hash: Some(body.hash()),
                timestamp: 0,
                seq_num,
                backlink,
                previous: vec![],
                extensions: Some(extensions),
            };
            header.sign(&private_key);

            let bytes = header.to_bytes();

            backlink = Some(header.hash());
            seq_num += 1;

            yield (bytes, Some(body.to_bytes()));
        }
    }
}
