// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use libfuzzer_sys::fuzz_target;
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Header, PrivateKey};

// Create arbitrary header, sign, serialize and deserialize it.
fuzz_target!(|header: Header<()>| {
    let private_key = PrivateKey::new();

    let mut header = header;
    header.sign(&private_key);
    header.verify();

    let bytes = encode_cbor(&header).expect("header encoding");
    let result: Result<Header<()>, _> = decode_cbor(&bytes[..]);

    // We expect these cases to fail:
    //
    // 1. Payload hash not expected when payload size is zero.
    // 2. Payload hash expected when payload size non-zero.
    // 3. Backlink not expected when seq number is zero.
    // 4. Backlink expected when seq number is non-zero.
    //
    // All other cases should successfully deserialize.
    if header.payload_size == 0 && header.payload_hash.is_some()
        || header.payload_size != 0 && header.payload_hash.is_none()
        || header.seq_num == 0 && header.backlink.is_some()
        || header.seq_num != 0 && header.backlink.is_none()
    {
        assert!(result.is_err())
    } else {
        let header_again = result.expect("header decoding");
        header_again.verify();
        assert_eq!(header, header_again)
    }
});
