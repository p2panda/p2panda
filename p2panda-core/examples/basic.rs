//! A minimal example showing basic usage of the core p2panda types.
//!
//! We create a new body containing a data payload, as well as a header, and then sign the header
//! with an Ed25519 private key. The signed header and body are then used to create an operation.
//! Finally, we validate the operation.
use p2panda_core::{Body, Header, Operation, SigningKey, validate_operation};

fn main() {
    // Create a new Ed25519 signing key.
    let signing_key = SigningKey::generate();

    // An operation body contains application data.
    let body = Body::from_bytes("Hello, Sloth!".as_bytes());

    // Create and sign a header.
    let header = Header::builder().body(&body).build(&signing_key, ());

    // An operation containing the header hash (the operation id), the header itself and an optional body.
    let operation = Operation {
        hash: header.hash(),
        header,
        body: Some(body),
    };

    // Validate the header and, when included, that the body matches the `payload_hash`.
    assert!(validate_operation(&operation).is_ok());
}
