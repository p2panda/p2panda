// SPDX-License-Identifier: AGPL-3.0-or-later

/// Hard-coded fixtures with valid and invalid byte encodings for testing.
use rstest::fixture;

use crate::entry::{decode_entry, Entry, EntrySigned};
use crate::identity::KeyPair;
use crate::operation::OperationEncoded;
use crate::schema::{FieldType, Schema};
use crate::test_utils::constants::TEST_SCHEMA_ID;
use crate::test_utils::fixtures::key_pair;

use super::{schema, schema_item};

/// Fixture struct which contains versioned p2panda data for testing.
#[derive(Debug)]
pub struct Fixture {
    pub entry: Entry,
    pub entry_signed_encoded: EntrySigned,
    pub key_pair: KeyPair,
    pub operation_encoded: OperationEncoded,
    pub schema: Schema,
}

/// Fixture which injects p2panda testing data from p2panda version `0.3.0`.
#[fixture]
pub fn v0_3_0_fixture() -> Fixture {
    let schema = schema_item(
        schema(TEST_SCHEMA_ID),
        "schema for v0.3.0 version fixture",
        vec![
            ("name", FieldType::String),
            ("description", FieldType::String),
        ],
    );
    let key_pair = key_pair("4c21b14046f284f87f1ea4be4b973664221ad483079a68ed35a6812553b41176");
    let entry_signed_encoded = EntrySigned::new("009cdb3a8c0c4b308173d4c3c43a67a6d013444af99acb8be6c52423746d9aa2c101019c0020b3e009d679d3a25fe49bafc88f5e13bd86b1ad2823c0e95c78cf090518fb87b5c5d9cda0542abd65b04ecf668764b50370eb049e21aa043e7355b010e12342cb3023a6f04de769df07879b6f36d6951fb19dbae3fa928d26e71b5c30de578103").unwrap();
    let operation_encoded = OperationEncoded::new("a466616374696f6e6663726561746566736368656d61784a76656e75655f30303230633635353637616533376566656132393365333461396337643133663866326266323364626463336235633762396162343632393331313163343866633738626776657273696f6e01666669656c6473a26b6465736372697074696f6e71666f7220706c6179696e67206368657373646e616d65656368657373").unwrap();
    let entry = decode_entry(
        &entry_signed_encoded,
        Some(&operation_encoded),
        Some(&schema),
    )
    .unwrap();

    // Comment out to regenerate fixture:
    // use std::convert::TryFrom;
    // use crate::operation::OperationValue;
    // use crate::test_utils::fixtures::{create_operation, entry, operation_fields};
    // let operation_fields = vec![
    //     ("name", OperationValue::Text("chess".to_string())),
    //     (
    //         "description",
    //         OperationValue::Text("for playing chess".to_string()),
    //     ),
    // ];
    // let operation = create_operation(&operation_fields);
    // let key_pair = key_pair("4c21b14046f284f87f1ea4be4b973664221ad483079a68ed35a6812553b41176");
    // let operation = create_operation(&operation_fields);
    // let entry_signed_encoded =
    //     crate::entry::sign_and_encode(&entry(1, 1, None, None, Some(operation.clone())), &key_pair)
    //         .unwrap();
    // println!("{:?}", entry_signed_encoded.as_str());
    // println!("{:?}", OperationEncoded::try_from(&operation).unwrap());

    Fixture {
        entry_signed_encoded,
        operation_encoded,
        key_pair,
        entry,
        schema,
    }
}
