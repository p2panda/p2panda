// SPDX-License-Identifier: AGPL-3.0-or-later

/// General purpose fixtures which can be injected into rstest methods as parameters.
///
/// The fixtures can optionally be passed in with custom parameters which overides the default values.
use rstest::fixture;

use crate::entry::{Entry, EntrySigned, SeqNum};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::operation::{Operation, OperationEncoded, OperationFields};
use crate::test_utils::utils::{self, DEFAULT_HASH, DEFAULT_PRIVATE_KEY, DEFAULT_SCHEMA_HASH};

/// Fixture struct which contains versioned p2panda data for testing
#[derive(Debug)]
pub struct Fixture {
    pub entry: Entry,
    pub entry_signed_encoded: EntrySigned,
    pub key_pair: KeyPair,
    pub operation_encoded: OperationEncoded,
}

/// Fixture which injects the default private key string into a test method
#[fixture]
pub fn private_key() -> String {
    DEFAULT_PRIVATE_KEY.into()
}

/// Fixture which injects the default KeyPair into a test method. Default value can be overridden at testing
/// time by passing in a custom private key string.
#[fixture]
pub fn key_pair(private_key: String) -> KeyPair {
    utils::keypair_from_private(private_key)
}

/// Fixture which injects the default SeqNum into a test method. Default value can be overridden at testing
/// time by passing in a custom seq num as i64.
#[fixture]
pub fn seq_num(#[default(1)] n: i64) -> SeqNum {
    utils::seq_num(n)
}

/// Fixture which injects the default schema Hash into a test method. Default value can be overridden at testing
/// time by passing in a custom schema hash string.
#[fixture]
pub fn schema(#[default(DEFAULT_SCHEMA_HASH)] schema_str: &str) -> Hash {
    utils::hash(schema_str)
}

/// Fixture which injects the default Hash into a test method. Default value can be overridden at testing
/// time by passing in a custom hash string.
#[fixture]
pub fn hash(#[default(DEFAULT_HASH)] hash_str: &str) -> Hash {
    utils::hash(hash_str)
}

/// Fixture which injects the default OperationFields value into a test method. Default value can be overridden at testing
/// time by passing in a custom vector of key-value tuples.
#[fixture]
pub fn fields(
    #[default(vec![("message", "Hello!")])] fields_vec: Vec<(&str, &str)>,
) -> OperationFields {
    utils::operation_fields(fields_vec)
}

/// Fixture which injects the default Entry into a test method. Default value can be overridden at testing
/// time by passing in custom operation, seq number, backlink and skiplink.
#[fixture]
pub fn entry(
    operation: Operation,
    seq_num: SeqNum,
    #[default(None)] backlink: Option<Hash>,
    #[default(None)] skiplink: Option<Hash>,
) -> Entry {
    utils::entry(operation, skiplink, backlink, seq_num)
}

/// Fixture which injects the default Operation into a test method. Default value can be overridden at testing
/// time by passing in custom operation fields and instance id.
#[fixture]
pub fn operation(
    #[default(Some(fields(vec![("message", "Hello!")])))] fields: Option<OperationFields>,
    #[default(None)] instance_id: Option<Hash>,
) -> Operation {
    utils::any_operation(fields, instance_id)
}

/// Fixture which injects the default Hash into a test method as an Option. Default value can be overridden at testing
/// time by passing in custom hash string.
#[fixture]
pub fn some_hash(#[default(DEFAULT_HASH)] str: &str) -> Option<Hash> {
    let hash = Hash::new(str);
    Some(hash.unwrap())
}

/// Fixture which injects the default CREATE Operation into a test method. Default value can be overridden at testing
/// time by passing in custom schema hash and operation fields.
#[fixture]
pub fn create_operation(schema: Hash, fields: OperationFields) -> Operation {
    utils::create_operation(schema, fields)
}

/// Fixture which injects the default UPDATE Operation into a test method. Default value can be overridden at testing
/// time by passing in custom schema hash, instance id hash and operation fields.
#[fixture]
pub fn update_operation(
    schema: Hash,
    #[from(hash)] instance_id: Hash,
    #[default(fields(vec![("message", "Updated, hello!")]))] fields: OperationFields,
) -> Operation {
    utils::update_operation(schema, instance_id, fields)
}

/// Fixture which injects the default DELETE Operation into a test method. Default value can be overridden at testing
/// time by passing in custom schema hash and instance id hash.
#[fixture]
pub fn delete_operation(schema: Hash, #[from(hash)] instance_id: Hash) -> Operation {
    utils::delete_operation(schema, instance_id)
}

/// Fixture which injects p2panda testing data from p2panda version 0.3.0
#[fixture]
pub fn v0_3_0_fixture() -> Fixture {
    let operation_fields = utils::operation_fields(vec![
        ("name", "chess"),
        ("description", "for playing chess"),
    ]);
    let operation = create_operation(Hash::new(DEFAULT_SCHEMA_HASH).unwrap(), operation_fields);
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
