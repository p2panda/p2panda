#![no_main]

use p2panda_core::operation::Header;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|header: Header<String>| {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(&header, &mut bytes).unwrap();
    let header_again: Header<String> = ciborium::de::from_reader(&bytes[..]).unwrap();
    assert_eq!(header, header_again);
});
