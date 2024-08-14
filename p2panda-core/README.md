<h1 align="center">p2panda-core</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>All the things a panda needs</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://github.com/p2panda/p2panda/releases">
      Releases
    </a>
    <span> | </span>
    <a href="https://p2panda.org/about/contribute">
      Contribute
    </a>
    <span> | </span>
    <a href="https://p2panda.org">
      Website
    </a>
  </h3>
</div>

p2panda core types based on the new [namakemono](https://p2panda.org/specifications/namakemono/) specification. 🦥 

## Features

* BLAKE3 `Hash`
* Ed25519 `PrivateKey`, `PublicKey` and `Signature`
* p2panda `Operation`, `Header`, `Body` and some validation methods
* CBOR based encoding with `serde` and `ciborium`

## Examples

### Create and sign an operation

```rust
// Ed25519 signing key
let private_key = PrivateKey::new();

// Operation body contains application data
let body = Body::new("Hello, Sloth!".as_bytes());

// Create header
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
    extensions: None,
};

// Sign header
header.sign(&private_key);

// An operation containing the header hash (the operation id), the header itself and an optional body
let operation = Operation {
    hash: header.hash(),
    header,
    body: Some(body),
};

// Validate the header and, when included, that the body matches the `payload_hash`
validate_operation(&operation).is_ok();
```

### Add extensions to operation

```rust
// Define custom extension types required for your application
#[derive(Clone, Serialize, Deserialize)]
struct LogId(u64);

#[derive(Clone, Serialize, Deserialize)]
struct Expiry(u64);

#[derive(Clone, Serialize, Deserialize)]
struct CustomExtensions {
    log_id: LogId,
    expires: Expiry,
}

// Implement the `Extension` trait for all unique types
impl Extension<LogId> for CustomExtensions {
    fn extract(&self) -> Option<LogId> {
        Some(self.log_id.to_owned())
    }
}

impl Extension<Expiry> for CustomExtensions {
    fn extract(&self) -> Option<Expiry> {
        Some(self.expires.to_owned())
    }
}

// Create an concrete extension, this will be encoded on a `Header`
let extensions = CustomExtensions {
    log_id: LogId(0),
    expires: Expiry(0123456),
};

// Extract the required fields by their type
let log_id = Extension::<LogId>::extract(&extensions).unwrap();
let expiry = Extension::<Expiry>::extract(&extensions).unwrap();
```
