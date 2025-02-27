//! A minimal example showing basic usage of the core p2panda types.
//!
//! We create a new body containing a data payload, as well as a header, and then sign the header
//! with an Ed25519 private key. The signed header and body are then used to create an operation.
//! Finally, we validate the operation.
use p2panda_core::{Body, Header, Operation, PrivateKey, validate_operation};

fn main() {
    // Create a new Ed25519 signing key.
    let private_key = PrivateKey::new();

    // An operation body contains application data.
    let body = Body::new("Hello, Sloth!".as_bytes());

    // Create a header.
    let mut header = Header {
        version: 1,
        public_key: private_key.public_key(),
        signature: None,
        payload_size: body.size(),
        payload_hash: Some(body.hash()),
        timestamp: 0,
        seq_num: 0,
        backlink: None,
        previous: vec![],
        extensions: None::<()>,
    };

    // Sign the header.
    header.sign(&private_key);

    // An operation containing the header hash (the operation id), the header itself and an optional body.
    let operation = Operation {
        hash: header.hash(),
        header,
        body: Some(body),
    };

    // Validate the header and, when included, that the body matches the `payload_hash`.
    assert!(validate_operation(&operation).is_ok());
}
