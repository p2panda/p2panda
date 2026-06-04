//! A minimal example using the extensions field of an operation header to add and extract custom
//! data.
//!
//! We define a custom struct to store a log ID and an expiry timestamp and then implement the
//! `Extension` trait to define the means of extracting each value from a p2panda header.
use serde::{Deserialize, Serialize};

use p2panda_core::{Extension, Hash, Header, SigningKey};

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

        header.extensions.log_id.clone()
    }
}

impl Extension<Expiry> for CustomExtensions {
    fn extract(header: &Header<Self>) -> Option<Expiry> {
        Some(header.extensions.expires.clone())
    }
}

fn main() {
    let extensions = CustomExtensions {
        log_id: None,
        expires: Expiry(0123456),
    };

    let signing_key = SigningKey::generate();

    let header = Header::builder()
        .body("Hello, Sloth".as_bytes())
        .build(&signing_key, extensions.clone());

    let log_id: LogId = header.extension().unwrap();
    let expiry: Expiry = header.extension().unwrap();

    assert_eq!(header.hash(), log_id.0);
    assert_eq!(extensions.expires.0, expiry.0);
}
