// SPDX-License-Identifier: AGPL-3.0-or-later

use p2panda_rs::identity_v2::KeyPair;
use p2panda_rs::operation_v2::body::encode::encode_body;
use p2panda_rs::operation_v2::header::encode::encode_header;
use p2panda_rs::operation_v2::OperationBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Id of the schema which describes the data we want to publish. This should
    // already be known to the node we are publishing to.
    pub const SCHEMA_ID: &str =
        "profile_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b";

    // Generate new Ed25519 key pair.
    let key_pair = KeyPair::new();

    // Build and sign a CREATE operation.
    let operation = OperationBuilder::new(&SCHEMA_ID.parse()?)
        .fields(&[("username", "panda".into())])
        .sign(&key_pair)?;

    // Encode operation header and body.
    let encoded_header = encode_header(operation.header())?;
    let encoded_body = encode_body(operation.body())?;

    println!("{} {}", encoded_header, encoded_body);

    Ok(())
}
