// SPDX-License-Identifier: AGPL-3.0-or-later

/// Hard-coded fixtures with valid and invalid byte encodings for testing.
use rstest::fixture;

use crate::entry::{Entry, EntrySigned};
use crate::identity::KeyPair;
use crate::operation::{OperationEncoded, OperationValue};
use crate::schema::SchemaId;
use crate::test_utils::constants::TEST_SCHEMA_ID;
use crate::test_utils::fixtures::{create_operation, entry, seq_num};
use crate::test_utils::utils;

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
    let operation_fields = utils::operation_fields(vec![
        ("name", OperationValue::Text("chess".to_string())),
        (
            "description",
            OperationValue::Text("for playing chess".to_string()),
        ),
    ]);
    let operation = create_operation(SchemaId::new(TEST_SCHEMA_ID).unwrap(), operation_fields);
    let key_pair = utils::keypair_from_private(
        "4c21b14046f284f87f1ea4be4b973664221ad483079a68ed35a6812553b41176".into(),
    );

    // Comment out to regenerate fixture:
    // use std::convert::TryFrom;
    // let entry_signed_encoded =
    //     crate::entry::sign_and_encode(&entry(operation.clone(), seq_num(1), None, None), &key_pair)
    //         .unwrap();
    // println!("{:?}", entry_signed_encoded.as_str());
    // println!("{:?}", OperationEncoded::try_from(&operation).unwrap());

    Fixture {
        entry_signed_encoded: EntrySigned::new("009cdb3a8c0c4b308173d4c3c43a67a6d013444af99acb8be6c52423746d9aa2c10101a6002064a570b7989c71973f186931c009e4ba7fa8cf72a33732a3f82b2a91dca4a08962e6b0b9b435600e4190e89a536060e62340ac3411e2e1f1d9ba8e61b531cc195ff37bcbee544b55a2f4bd213ff35762174f6d23a19a74b9f1ffbb5ccbf38e00").unwrap(),
        operation_encoded: OperationEncoded::new("a466616374696f6e6663726561746566736368656d61784a76656e75655f30303230633635353637616533376566656132393365333461396337643133663866326266323364626463336235633762396162343632393331313163343866633738626776657273696f6e01666669656c6473a26b6465736372697074696f6ea16373747271666f7220706c6179696e67206368657373646e616d65a163737472656368657373").unwrap(),
        key_pair,
        entry: entry(operation, seq_num(1), None, None)
    }
}

/// Invalid YASMF hash in `document` with correct length but unknown hash format identifier.
#[fixture]
pub fn operation_encoded_invalid_relation_fields() -> OperationEncoded {
    // {
    //   "action": "create",
    //   "schema": "venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b",
    //   "version": 1,
    //   "fields": {
    //     "locations": {
    //       "relation": "83e2043738f2b5cdcd3b6cb0fbb82fe125905d0f75e16488a38d395ff5f9d5ea82b5"
    //     }
    //   }
    // }
    OperationEncoded::new("A466616374696F6E6663726561746566736368656D61784A76656E75655F30303230633635353637616533376566656132393365333461396337643133663866326266323364626463336235633762396162343632393331313163343866633738626776657273696F6E01666669656C6473A1696C6F636174696F6E73A16872656C6174696F6E78443833653230343337333866326235636463643362366362306662623832666531323539303564306637356531363438386133386433393566663566396435656138326235").unwrap()
}
