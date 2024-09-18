// SPDX-License-Identifier: AGPL-3.0-or-later

use async_stream::stream;
use futures_util::Stream;
use p2panda_core::extensions::DefaultExtensions;
use p2panda_core::{Body, Header, PrivateKey};

use crate::operation::RawOperation;

pub fn mock_stream() -> impl Stream<Item = RawOperation> {
    let private_key = PrivateKey::new();
    let body = Body::new(b"Hello, Penguin!");

    let mut backlink = None;
    let mut seq_num = 0;

    stream! {
        loop {
            let mut header = Header::<DefaultExtensions> {
                public_key: private_key.public_key(),
                version: 1,
                signature: None,
                payload_size: body.size(),
                payload_hash: Some(body.hash()),
                timestamp: 0,
                seq_num,
                backlink,
                previous: vec![],
                extensions: None,
            };
            header.sign(&private_key);

            let bytes = header.to_bytes();

            backlink = Some(header.hash());
            seq_num += 1;

            yield (bytes, Some(body.to_bytes()));
        }
    }
}
