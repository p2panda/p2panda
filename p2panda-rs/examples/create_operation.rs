// SPDX-License-Identifier: AGPL-3.0-or-later

use p2panda_rs::entry::encode::encode_entry;
use p2panda_rs::entry::EntryBuilder;
use p2panda_rs::identity::KeyPair;
use p2panda_rs::operation::encode::encode_operation;
use p2panda_rs::operation::OperationBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Id of the schema which describes the data we want to publish. This should
    // already be known to the node we are publishing to.
    pub const SCHEMA_ID_STR: &str =
        "profile_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b";

    // Generate new Ed25519 key pair.
    let key_pair = KeyPair::new();

    // Add field data to "create" operation.
    let operation = OperationBuilder::new(&SCHEMA_ID_STR.parse()?)
        .fields(&[("username", "panda".into())])
        .build()?;

    // Encode operation into bytes.
    let encoded_operation = encode_operation(&operation)?;

    // Create Bamboo entry (append-only log data type) with operation as payload.
    let entry = EntryBuilder::new().sign(&encoded_operation, &key_pair)?;

    // Encode entry into bytes.
    let encoded_entry = encode_entry(&entry)?;

    println!("{} {}", encoded_entry, encoded_operation);

    Ok(())
}
