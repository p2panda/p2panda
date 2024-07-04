#![no_main]

use p2panda_core::{Header, PrivateKey, SignedHeader};

use libfuzzer_sys::fuzz_target;

fuzz_target!(|header: Header<String>| {
    let private_key = PrivateKey::new();

    // Sign header
    let signed_header = header.sign(&private_key);
    signed_header.verify();

    // Serialize signed header
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(&signed_header, &mut bytes).unwrap();

    // Deserialize signed header bytes again
    let result: Result<SignedHeader<String>, _> = ciborium::de::from_reader(&bytes[..]);

    // We expect these cases to fail
    if header.payload_size == 0 && header.payload_hash.is_some() // payload hash not expected when payload size is zero
        || header.payload_size != 0 && header.payload_hash.is_none() // payload hash expected when payload size non-zero
        || header.seq_num == 0 && header.backlink.is_some() // backlink not expected when seq number is zero
        || header.seq_num != 0 && header.backlink.is_none()
    // backlink expected when seq number is non-zero
    {
        // All these cases should error
        assert!(result.is_err())
    } else {
        // All other cases should successfully deserialize
        let signed_header_again = result.unwrap();

        // Verify the signed header
        signed_header_again.verify();

        // Assert it matches the original
        assert_eq!(signed_header, signed_header_again)
    }
});
