// SPDX-License-Identifier: AGPL-3.0-or-later

//! Hard-coded fixtures with valid and invalid byte encodings for testing.
use rstest::fixture;

use crate::entry::decode::decode_entry;
use crate::entry::{EncodedEntry, Entry};
use crate::identity::KeyPair;
use crate::operation::EncodedOperation;
use crate::schema::{FieldType, Schema};
use crate::test_utils::constants::SCHEMA_ID;
use crate::test_utils::fixtures::{key_pair, schema_id};

/// Fixture struct which contains versioned p2panda data for testing.
#[derive(Debug)]
pub struct Fixture {
    pub entry: Entry,
    pub entry_encoded: EncodedEntry,
    pub key_pair: KeyPair,
    pub operation_encoded: EncodedOperation,
    pub schema: Schema,
}

/// Fixture which injects p2panda testing data from the latest p2panda version.
#[fixture]
pub fn latest_fixture() -> Fixture {
    let key_pair = key_pair("4c21b14046f284f87f1ea4be4b973664221ad483079a68ed35a6812553b41176");

    // Hard-coded bytes for entry and operation
    let entry_encoded = EncodedEntry::from_str("009cdb3a8c0c4b308173d4c3c43a67a6d013444af99acb8be6c52423746d9aa2c10001790020d00b4a66f86b0868b948204bff9e17e1688040e895c2c1f0b3114f45d412978d44804f48e535f87936bf8574c287e470ee1bf453920cddabc244a1168ade39ad14c430d377977695839cbb31d30d8b6577caaf9ad759c3060bfbac0593b50502");

    let operation_encoded = EncodedOperation::from_str("840100784a76656e75655f3030323063363535363761653337656665613239336533346139633764313366386632626632336462646333623563376239616234363239333131316334386663373862a26b6465736372697074696f6e71666f7220706c6179696e67206368657373646e616d65656368657373");

    // Decode entry
    let entry = decode_entry(&entry_encoded).unwrap();

    // Initialise schema for this operation
    let schema_id = schema_id(SCHEMA_ID);
    let schema_description: &str = "Chess is fun!";

    let schema = Schema::new(
        &schema_id,
        &schema_description,
        vec![
            ("name", FieldType::String),
            ("description", FieldType::String),
        ],
    )
    .unwrap();

    // Comment out to regenerate fixture
    /* use crate::entry::{LogId, SeqNum};
    use crate::operation::encode::encode_operation;
    use crate::operation::OperationValue;
    use crate::test_utils::fixtures::create_operation;
    let operation = create_operation(
        vec![
            ("name", OperationValue::String("chess".to_string())),
            (
                "description",
                OperationValue::String("for playing chess".to_string()),
            ),
        ],
        schema.clone(),
    );
    let encoded_operation = encode_operation(&operation).unwrap();
    let encoded_entry = crate::entry::encode::sign_and_encode_entry(
        &LogId::default(),
        &SeqNum::default(),
        None,
        None,
        &encoded_operation,
        &key_pair,
    )
    .unwrap();
    println!("{}", encoded_entry);
    println!("{}", encoded_operation); */

    Fixture {
        entry,
        entry_encoded,
        key_pair,
        operation_encoded,
        schema,
    }
}
