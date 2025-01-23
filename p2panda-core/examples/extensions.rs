//! A minimal example using the extensions field of an operation header to add and extract custom
//! data.
//!
//! We define a custom struct to store a log ID and an expiry timestamp and then implement the
//! `Extension` trait to define the means of extracting each value from a p2panda header.
use serde::{Deserialize, Serialize};

use p2panda_core::{Body, Extension, Hash, Header, PrivateKey};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LogId(Hash);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Expiry(u64);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CustomExtensions {
    log_id: Option<LogId>,
    expires: Expiry,
}

impl Extension<LogId> for CustomExtensions {
    fn extract(header: &Header<Self>) -> Option<LogId> {
        if header.seq_num == 0 {
            return Some(LogId(header.hash()));
        };

        let Some(extensions) = header.extensions.as_ref() else {
            return None;
        };

        extensions.log_id.clone()
    }
}

impl Extension<Expiry> for CustomExtensions {
    fn extract(header: &Header<Self>) -> Option<Expiry> {
        header
            .extensions
            .as_ref()
            .map(|extensions| extensions.expires.clone())
    }
}

fn main() {
    let extensions = CustomExtensions {
        log_id: None,
        expires: Expiry(0123456),
    };

    let private_key = PrivateKey::new();
    let body: Body = Body::new("Hello, Sloth!".as_bytes());

    let mut header = Header {
        version: 1,
        public_key: private_key.public_key(),
        signature: None,
        payload_size: body.size(),
        payload_hash: Some(body.hash()),
        timestamp: 0,
        seq_num: 0,
        backlink: None,
        previous: vec![],
        extensions: Some(extensions.clone()),
    };

    header.sign(&private_key);

    let log_id: LogId = header.extension().unwrap();
    let expiry: Expiry = header.extension().unwrap();

    assert_eq!(header.hash(), log_id.0);
    assert_eq!(extensions.expires.0, expiry.0);
}
