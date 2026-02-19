// SPDX-License-Identifier: AGPL-3.0-or-later

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{Body, Extensions, Hash, Header, Operation, PrivateKey, Topic};

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

    pub fn id(&self) -> Topic {
        self.log_id
    }

    pub fn operation<E: Extensions>(&self, body: &[u8], extensions: E) -> Operation<E> {
        let body = Body::from(body);

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX EPOCH!")
            .as_secs();

        let mut seq_num = self.seq_num.borrow_mut();
        let mut backlink = self.backlink.borrow_mut();

        let mut header = Header::<E> {
            public_key: self.private_key.public_key(),
            version: 1,
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp,
            seq_num: *seq_num,
            backlink: *backlink,
            previous: vec![],
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
