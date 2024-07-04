#![no_main]

use p2panda_core::operation::SignedHeader;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|header: SignedHeader<String>| {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(&header, &mut bytes).unwrap();
    let header_again: SignedHeader<String> = ciborium::de::from_reader(&bytes[..]).unwrap();
    assert_eq!(header, header_again);
});
