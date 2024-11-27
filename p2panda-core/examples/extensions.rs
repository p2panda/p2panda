//! A minimal example using the extensions field of an operation header to add and extract custom data.
//!
//! We define a custom struct to store a log ID and an expiry timestamp and then implement the
//! `Extension` trait to define the means of extracting each value from a p2panda header.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use p2panda_core::{Body, Extension, Header, PrivateKey};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct LogId(u64);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Expiry(u64);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct CustomExtensions {
    log_id: LogId,
    expires: Expiry,
}

impl Extension<LogId> for CustomExtensions {
    fn extract(&self) -> Option<LogId> {
        Some(self.log_id.to_owned())
    }
}

impl Extension<Expiry> for CustomExtensions {
    fn extract(&self) -> Option<Expiry> {
        Some(self.expires.to_owned())
    }
}

fn main() -> Result<()> {
    let extensions = CustomExtensions {
        log_id: LogId(0),
        expires: Expiry(0123456),
    };

    let log_id = Extension::<LogId>::extract(&extensions).unwrap();
    let expiry = Extension::<Expiry>::extract(&extensions).unwrap();

    assert_eq!(extensions.log_id.0, log_id.0);
    assert_eq!(extensions.expires.0, expiry.0);

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

    // Thanks to blanket implementation of Extension<T> on Header we can extract the
    // extension value from the header itself.
    let log_id = Extension::<LogId>::extract(&header).unwrap();
    let expiry = Extension::<Expiry>::extract(&header).unwrap();

    assert_eq!(extensions.log_id.0, log_id.0);
    assert_eq!(extensions.expires.0, expiry.0);

    Ok(())
}
