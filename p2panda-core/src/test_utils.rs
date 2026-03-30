// SPDX-License-Identifier: AGPL-3.0-or-later

use std::cell::RefCell;
use std::rc::Rc;

use crate::timestamp::Timestamp;
use crate::{Body, Extensions, Hash, Header, Operation, PrivateKey, PublicKey, Topic};

#[derive(Clone, Default)]
pub struct TestLog {
    private_key: PrivateKey,
    backlink: Rc<RefCell<Option<Hash>>>,
    seq_num: Rc<RefCell<u64>>,
    log_id: Topic,
}

impl TestLog {
    pub fn new() -> Self {
        Self {
            private_key: PrivateKey::new(),
            backlink: Rc::default(),
            seq_num: Rc::default(),
            log_id: Topic::new(),
        }
    }

    pub fn from_private_key(private_key: PrivateKey) -> Self {
        let mut log = TestLog::new();
        log.private_key = private_key;
        log
    }

    pub fn id(&self) -> Topic {
        self.log_id
    }

    pub fn author(&self) -> PublicKey {
        self.private_key.public_key()
    }

    pub fn operation<E: Extensions>(&self, body: &[u8], extensions: E) -> Operation<E> {
        let body = Body::from(body);

        let mut seq_num = self.seq_num.borrow_mut();
        let mut backlink = self.backlink.borrow_mut();

        let mut header = Header::<E> {
            public_key: self.private_key.public_key(),
            version: 1,
            signature: None,
            payload_size: body.size(),
            payload_hash: if body.size() == 0 {
                None
            } else {
                Some(body.hash())
            },
            timestamp: Timestamp::now(),
            seq_num: *seq_num,
            backlink: *backlink,
            extensions,
        };
        header.sign(&self.private_key);

        *backlink = Some(header.hash());
        *seq_num += 1;

        Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Header;
    use crate::cbor::{decode_cbor, encode_cbor};
    use crate::test_utils::TestLog;

    #[test]
    fn zero_byte_body() {
        let log = TestLog::new();
        let operation = log.operation(&[], ());
        let bytes = encode_cbor(operation.header()).unwrap();
        assert!(decode_cbor::<Header, _>(&bytes[..]).is_ok());
    }
}
