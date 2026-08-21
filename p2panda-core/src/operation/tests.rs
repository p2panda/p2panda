// SPDX-License-Identifier: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};

use crate::identity::SigningKey;
use crate::traits::Provenance;
use crate::{AnyHeader, Body, Hash, Header, HeaderError};

#[test]
fn paths_leading_to_same_encoding() {
    let signing_key = SigningKey::generate();

    let header = Header::builder()
        .body(b"test")
        .chain(2, Hash::from([2; 32]))
        .build(&signing_key, ());

    let hacky_header = {
        let body = Body::from_bytes(b"test");
        let mut hacky_header = Header::<()> {
            verifying_key: signing_key.verifying_key(),
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            seq_num: 2,
            backlink: Some(Hash::from([2; 32])),
            ..Default::default()
        };
        hacky_header.sign(&signing_key);
        hacky_header
    };

    assert_eq!(header.encode(), hacky_header.encode());
    assert_eq!(header.verify(), hacky_header.verify());
    assert!(header.verify());
    assert_eq!(header.hash(), hacky_header.hash());
    assert_eq!(header.size(), hacky_header.size());

    let any_header = AnyHeader::decode(&header.encode()).unwrap();

    assert_eq!(header.encode(), any_header.encode());
    assert_eq!(header.verify(), any_header.verify());
    assert!(any_header.verify());
    assert_eq!(header.hash(), any_header.hash());
    assert_eq!(header.size(), any_header.size());

    let header_again = Header::<()>::try_from(any_header.clone()).unwrap();
    assert_eq!(header, header_again);
}

#[test]
fn any_header_conversions() {
    let signing_key = SigningKey::generate();

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    struct TestExtensions {
        field_a: Vec<u8>,
        field_b: bool,
        field_c: u64,
    }

    let header = Header::builder().body(b"hello").build(
        &signing_key,
        TestExtensions {
            field_a: vec![61, 112, 43],
            field_b: true,
            field_c: 54_938,
        },
    );

    let hash = header.hash();
    assert!(header.verify());

    let any_header = AnyHeader::try_from(header.clone()).unwrap();
    assert_eq!(any_header.hash(), hash);
    assert_eq!(any_header.size(), header.encode().len() as u32);

    let header_again: Header<TestExtensions> = any_header.try_into().unwrap();
    assert_eq!(header, header_again);
}

#[test]
fn any_header_errors() {
    let signing_key = SigningKey::generate();

    // First check that this header is valid. The body is b"test".
    let correct_header_bytes = r#"[
            1,
            h'6dec6975e5c280e9eadde785cf01df20690d02c8ab7a57efcd58150ab53a867f',
            h'a4331f8d5742d3c40b7e8cfb93e487f4b3b8878e64f7ec9985169d6ed3a3fa3b7e9c9d4b69d6c62e6079dba734705a4b8fc2210f2793bf1ee8b2af5963ce9e0b',
            4,
            h'4878ca0425c739fa427f7eda20fe845f6b2e46ba5fe2a14df5b1e32f50603215',
            0
        ]"#
        .parse::<cbor_core::Value>()
        .unwrap()
        .encode();
    assert!(AnyHeader::decode(&correct_header_bytes).is_ok());

    // Insufficient bytes for payload_hash.
    let invalid_header_bytes = r#"[
            1,
            h'6dec6975e5c280e9eadde785cf01df20690d02c8ab7a57efcd58150ab53a867f',
            h'a4331f8d5742d3c40b7e8cfb93e487f4b3b8878e64f7ec9985169d6ed3a3fa3b7e9c9d4b69d6c62e6079dba734705a4b8fc2210f2793bf1ee8b2af5963ce9e0b',
            4,
            h'4878',
            0
        ]"#
        .parse::<cbor_core::Value>()
        .unwrap()
        .encode();

    std::assert_matches!(
        AnyHeader::decode(&invalid_header_bytes),
        Err(HeaderError::InvalidBytesLen("payload_hash", 32, 2))
    );

    // Invalid signature.
    let mut header = Header::builder().build(&signing_key, ());
    header.verifying_key = SigningKey::generate().verifying_key();

    let result = AnyHeader::decode(&header.encode());
    std::assert_matches!(result, Err(HeaderError::InvalidSignature));

    // payload_size given without payload_hash.
    let mut header = Header::builder().build(&signing_key, ());
    header.payload_size = 2829099;
    header.sign(&signing_key);

    let result = AnyHeader::decode(&header.encode());
    std::assert_matches!(
        result,
        Err(HeaderError::UnexpectedFieldType(
            cbor_core::Error::IncompatibleType(cbor_core::DataType::Int),
            "payload_hash"
        ))
    );

    // payload_hash given without payload_size.
    let mut header = Header::<()> {
        verifying_key: signing_key.verifying_key(),
        payload_size: 0,
        payload_hash: Some(Hash::digest([0, 1, 2])),
        extensions: (),
        ..Default::default()
    };
    header.sign(&signing_key);

    let result = AnyHeader::decode(&header.encode());
    std::assert_matches!(
        result,
        Err(HeaderError::UnexpectedFieldType(
            cbor_core::Error::IncompatibleType(cbor_core::DataType::Bytes),
            "seq_num"
        ))
    );

    // backlink given with seq_num 0.
    let mut header = Header::<()> {
        verifying_key: signing_key.verifying_key(),
        seq_num: 0,
        backlink: Some(Hash::digest([0, 1, 2])),
        ..Default::default()
    };
    header.sign(&signing_key);

    // At this point we don't know that the backlink is _not_ an extension:
    let result = AnyHeader::decode(&header.encode()).expect("this is fine ..");

    // .. but latest here we'll find out!
    let result = Header::<()>::try_from(result);
    let result_2 = Header::<()>::decode(&header.encode());
    assert!(result.is_err());
    assert!(result_2.is_err());
    std::assert_matches!(result, Err(HeaderError::UnexpectedExtensions));

    // backlink not given with seq_num > 0.
    let mut header = Header::<()> {
        verifying_key: signing_key.verifying_key(),
        seq_num: 10,
        backlink: None,
        ..Default::default()
    };
    header.sign(&signing_key);

    let result = AnyHeader::decode(&header.encode());
    std::assert_matches!(result, Err(HeaderError::MissingField("backlink")));
}

#[test]
fn forwards_compatible_checks() {
    use crate::{PruneFlag, Timestamp};

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct LegacyExtensionsFormat {
        timestamp: Timestamp,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct FutureExtensionsFormat {
        timestamp: Timestamp,
        #[serde(default = "PruneFlag::default")]
        prune_flag: PruneFlag,
    }

    let signing_key = SigningKey::generate();

    let old_header = Header::builder().body(b"once upon a time").build(
        &signing_key,
        LegacyExtensionsFormat {
            timestamp: 1780572316919.into(),
        },
    );
    let old_header_bytes = old_header.encode();

    let new_header = Header::builder()
        .body(b"fitter, happier, more productive")
        .build(
            &signing_key,
            FutureExtensionsFormat {
                timestamp: 1780572316919.into(),
                prune_flag: true.into(),
            },
        );
    let new_header_bytes = new_header.encode();
    let new_header_hash = new_header.hash();

    // The old system can still parse headers with the new extensions format:
    let any_header = AnyHeader::decode(&new_header_bytes).unwrap();

    // The signature was checked during decoding already.
    assert!(any_header.verify());

    // .. and the hash digest matches with the original even though we don't know the new
    // extension format:
    assert_eq!(new_header_hash, any_header.hash());

    // It can even parse the extensions, will omit the unknown prune_flag field.
    let header = Header::<LegacyExtensionsFormat>::try_from(any_header).unwrap();
    assert_eq!(header.extensions.timestamp, 1780572316919.into());
    assert_eq!(new_header_hash, header.hash());
    assert!(header.verify());

    // The old system can still parse headers with the new extensions format, not too important
    // for this test, but nice to show:
    let header = Header::<FutureExtensionsFormat>::decode(&old_header_bytes).unwrap();
    assert_eq!(header.extensions.timestamp, 1780572316919.into());
    assert_eq!(header.extensions.prune_flag, false.into()); // set to default when not given
}

#[test]
fn non_canonical_extensions() {
    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct MyExtensions {
        field_a: u8,
        field_b: u8,
    }

    // [
    //   1,
    //   h'...',
    //   h'...',
    //   0,
    //   0,
    //   {
    //      "field_a": 12,
    //      "field_b": 17,
    //   }
    // ]
    let bytes = hex::decode("860158208b8a1a22ce4d22984c5eca66cb55d5a2679f42b7667e9b7838a15d0049b2bcea58409053796ef0724b493d2ddf0c240504ca7a8d3b80af5cd7eaf633622d7dea0469949cc4252017341171d8cdbaad2829aa27754425e041d198027a7c48150d2b0e0000a2676669656c645f610c676669656c645f6211").unwrap();

    // [
    //   1,
    //   h'...',
    //   h'...',
    //   0,
    //   0,
    //   {
    //      "field_b": 17, <-- the order of fields changed
    //      "field_a": 12,
    //   }
    // ]
    let non_canonical_bytes = hex::decode("860158208b8a1a22ce4d22984c5eca66cb55d5a2679f42b7667e9b7838a15d0049b2bcea58409053796ef0724b493d2ddf0c240504ca7a8d3b80af5cd7eaf633622d7dea0469949cc4252017341171d8cdbaad2829aa27754425e041d198027a7c48150d2b0e0000a2676669656c645f6211676669656c645f6111").unwrap();

    let result = Header::<MyExtensions>::decode(&bytes);
    assert!(result.is_ok(), "decoding canonical representation works");

    // Decoding the non-canonical version should fail on CBOR decoder level (and _not_ when we check
    // the integrity of the header since the signature is technically correct).
    let result = Header::<MyExtensions>::decode(&non_canonical_bytes);
    std::assert_matches!(result, Err(HeaderError::DecodingExtensions(_)));
}
