// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::operation::{Encode, Header};

#[cfg(feature = "cbor")]
impl Encode for Header {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        ciborium::ser::into_writer(&self, &mut bytes)
            // We can be sure that all values in this module are serializable and _if_ ciborium
            // still fails then because of something really bad ..
            .expect("CBOR encoder failed due to an critical IO error");

        bytes
    }
}
