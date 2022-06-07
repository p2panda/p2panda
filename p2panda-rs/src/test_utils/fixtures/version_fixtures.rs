// SPDX-License-Identifier: AGPL-3.0-or-later

/// Hard-coded fixtures with valid and invalid byte encodings for testing.
use rstest::fixture;

use crate::entry::{decode_entry, Entry, EntrySigned};
use crate::identity::KeyPair;
use crate::operation::OperationEncoded;
use crate::test_utils::fixtures::key_pair;

/// Fixture struct which contains versioned p2panda data for testing.
#[derive(Debug)]
pub struct Fixture {
    pub entry: Entry,
    pub entry_signed_encoded: EntrySigned,
    pub key_pair: KeyPair,
    pub operation_encoded: OperationEncoded,
}

/// Fixture which injects p2panda testing data from p2panda version `0.3.0`.
#[fixture]
pub fn v0_3_0_fixture() -> Fixture {
    let key_pair = key_pair("4c21b14046f284f87f1ea4be4b973664221ad483079a68ed35a6812553b41176");
    let entry_signed_encoded = EntrySigned::new("009cdb3a8c0c4b308173d4c3c43a67a6d013444af99acb8be6c52423746d9aa2c10101bc00207597aa680a7f619d72ec4410bb3a0af4bcb66509e43c1ddec70beefd4b158f5c1a0836975a8a23d92e7ff3d23742dfb4a5c447b2ef7b86fd1063743ba6bb50e0ddceff7d3814825aaf35cf1d2288061fcfff00375b91dcfc38f945a798d1810a").unwrap();
    let operation_encoded = OperationEncoded::new("a466616374696f6e6663726561746566736368656d61784a76656e75655f30303230633635353637616533376566656132393365333461396337643133663866326266323364626463336235633762396162343632393331313163343866633738626776657273696f6e01666669656c6473a26b6465736372697074696f6ea26474797065637374726576616c756571666f7220706c6179696e67206368657373646e616d65a26474797065637374726576616c7565656368657373").unwrap();
    let entry = decode_entry(&entry_signed_encoded, Some(&operation_encoded)).unwrap();

    // Comment out to regenerate fixture:
    /* use std::convert::TryFrom;
    let operation_fields = operation_fields(vec![
        ("name", OperationValue::Text("chess".to_string())),
        (
            "description",
            OperationValue::Text("for playing chess".to_string()),
        ),
    ]);
    let operation = create_operation(SchemaId::new(TEST_SCHEMA_ID).unwrap(), operation_fields);
    let key_pair = key_pair("4c21b14046f284f87f1ea4be4b973664221ad483079a68ed35a6812553b41176");
    let operation = create_operation(SchemaId::new(TEST_SCHEMA_ID).unwrap(), operation_fields);
    let entry_signed_encoded = crate::entry::sign_and_encode(
        &entry(operation.clone(), seq_num(1), None, None, log_id(1)),
        &key_pair,
    ).unwrap();
    println!("{:?}", entry_signed_encoded.as_str());
    println!("{:?}", OperationEncoded::try_from(&operation).unwrap()); */

    Fixture {
        entry_signed_encoded,
        operation_encoded,
        key_pair,
        entry,
    }
}
