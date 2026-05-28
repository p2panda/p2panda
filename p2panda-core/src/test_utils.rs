// SPDX-License-Identifier: AGPL-3.0-or-later

use std::cell::RefCell;
use std::rc::Rc;

use tracing_subscriber;

use crate::{Body, Extensions, Hash, Header, Operation, SeqNum, SigningKey, Topic, VerifyingKey};

pub fn setup_logging() {
    if std::env::var("RUST_LOG").is_ok() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();
    }
}

#[derive(Clone, Default)]
pub struct TestLog {
    signing_key: SigningKey,
    backlink: Rc<RefCell<Option<Hash>>>,
    seq_num: Rc<RefCell<SeqNum>>,
    log_id: Topic,
}

impl TestLog {
    pub fn new() -> Self {
        Self {
            signing_key: SigningKey::generate(),
            backlink: Rc::default(),
            seq_num: Rc::default(),
            log_id: Topic::random(),
        }
    }

    pub fn from_signing_key(signing_key: SigningKey) -> Self {
        let mut log = TestLog::new();
        log.signing_key = signing_key;
        log
    }

    pub fn id(&self) -> Topic {
        self.log_id
    }

    pub fn author(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    pub fn operation<E: Extensions>(&self, body: &[u8], extensions: E) -> Operation<E> {
        let body = Body::from(body);

        let mut seq_num = self.seq_num.borrow_mut();
        let mut backlink = self.backlink.borrow_mut();

        let mut header = Header::<E> {
            verifying_key: self.signing_key.verifying_key(),
            version: 1,
            signature: None,
            payload_size: body.size(),
            payload_hash: if body.size() == 0 {
                None
            } else {
                Some(body.hash())
            },
            seq_num: *seq_num,
            backlink: *backlink,
            extensions,
        };
        header.sign(&self.signing_key);

        *backlink = Some(header.hash());
        *seq_num += 1;

        Operation {
            hash: header.hash(),
            header,
            body: if body.size() == 0 { None } else { Some(body) },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Header;
    use crate::cbor::{decode_cbor, encode_cbor};

    use super::TestLog;

    #[test]
    fn zero_byte_body() {
        let log = TestLog::new();
        let operation = log.operation(&[], ());
        let bytes = encode_cbor(operation.header()).unwrap();
        assert!(decode_cbor::<Header, _>(&bytes[..]).is_ok());
    }
}
