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

        let mut header = Header::<E>::builder().body(&body);

        if let Some(backlink) = *backlink {
            header = header.chain(*seq_num, backlink);
        }

        let header = header.build(&self.signing_key, extensions);

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
    use crate::{AnyHeader, Header, SigningKey};

    #[test]
    fn zero_byte_body() {
        let signing_key = SigningKey::generate();
        let header = Header::builder().body(&[]).build(&signing_key, ());

        // Assure that setting an _empty_ body is equals having no body at all.
        assert_eq!(header.payload_size, 0);
        assert!(header.payload_hash.is_none());

        // Decoding works.
        let bytes = header.encode();
        assert!(AnyHeader::decode(&bytes).is_ok());
    }
}
