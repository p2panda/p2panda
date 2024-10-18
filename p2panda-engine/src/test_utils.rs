// SPDX-License-Identifier: AGPL-3.0-or-later

use async_stream::stream;
use futures_util::Stream;
use p2panda_core::{Body, Extension, Header, PrivateKey, RawOperation};
use serde::{Deserialize, Serialize};

use crate::extensions::{PruneFlag, StreamName};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Extensions {
    pub stream_name: StreamName,
    pub prune_flag: PruneFlag,
}

impl Extension<StreamName> for Extensions {
    fn extract(&self) -> Option<StreamName> {
        Some(self.stream_name.clone())
    }
}

impl Extension<PruneFlag> for Extensions {
    fn extract(&self) -> Option<PruneFlag> {
        Some(self.prune_flag.clone())
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
