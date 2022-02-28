// SPDX-License-Identifier: AGPL-3.0-or-later

/// Hard-coded fixtures with valid and invalid byte encodings for testing.
use rstest::fixture;

use crate::entry::{Entry, EntrySigned};
use crate::identity::KeyPair;
use crate::operation::{OperationEncoded, OperationValue};
use crate::schema::SchemaId;
use crate::test_utils::constants::DEFAULT_SCHEMA_HASH;
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
    let operation = create_operation(
        SchemaId::new(DEFAULT_SCHEMA_HASH).unwrap(),
        operation_fields,
    );
    let key_pair = utils::keypair_from_private(
        "4c21b14046f284f87f1ea4be4b973664221ad483079a68ed35a6812553b41176".into(),
    );

    // Comment out to regenerate fixture:
    // let entry_signed_encoded = sign_and_encode(
    //     &entry(operation.clone(), seq_num(1), None, None),
    //     &key_pair,
    // ).unwrap();
    // println!("{:?}", entry_signed_encoded.as_str());
    // println!("{?}", OperationEncoded::try_from(&operation)).unwrap();

    Fixture {
        entry_signed_encoded: EntrySigned::new("009cdb3a8c0c4b308173d4c3c43a67a6d013444af99acb8be6c52423746d9aa2c10101b6002065c34e1997b82fd08fc886bf6c2803cfaf93e3ad4da9128a661eb79e30f97bee25e525e72c99394ec91c33195f6b43c78274bd3096938260d5e18f237b57211d0a8e9eee49f594c1ddfb609ee0f9d0f502bf8701c3b2e5b0c34c61ec3e614a02").unwrap(),
        operation_encoded: OperationEncoded::new("a466616374696f6e6663726561746566736368656d61784430303230633635353637616533376566656132393365333461396337643133663866326266323364626463336235633762396162343632393331313163343866633738626776657273696f6e01666669656c6473a26b6465736372697074696f6ea26474797065637374726576616c756571666f7220706c6179696e67206368657373646e616d65a26474797065637374726576616c7565656368657373").unwrap(),
        key_pair,
        entry: entry(operation, seq_num(1), None, None)
    }
}

/// Invalid YASMF hash in `document` with correct length but unknown hash format identifier.
#[fixture]
pub fn operation_encoded_invalid_relation_fields() -> OperationEncoded {
    // {
    //   "action": "create",
    //   "schema": "0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b",
    //   "version": 1,
    //   "fields": {
    //     "locations": {
    //       "type": "relation",
    //       "value": "83e2043738f2b5cdcd3b6cb0fbb82fe125905d0f75e16488a38d395ff5f9d5ea82b5"
    //     }
    //   }
    // }
    OperationEncoded::new("A466616374696F6E6663726561746566736368656D61784430303230633635353637616533376566656132393365333461396337643133663866326266323364626463336235633762396162343632393331313163343866633738626776657273696F6E01666669656C6473A1696C6F636174696F6E73A264747970656872656C6174696F6E6576616C756578443833653230343337333866326235636463643362366362306662623832666531323539303564306637356531363438386133386433393566663566396435656138326235").unwrap()
}
