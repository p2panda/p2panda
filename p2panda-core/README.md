<h1 align="center">p2panda</h1>

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

p2panda core types based on the new [namakemono specification](https://p2panda.org/specifications/namakemono/). 🦥 

* BLAKE3 `Hash`
* Ed25519 `PrivateKey`, `PublicKey` and `Signature`
* p2panda `Operation`, `Header`, `Body` and some validation methods
* CBOR based encoding with `serde` and `ciborium`

## `Header` and `Body`

```rust
// Ed25519 signing key
let private_key = PrivateKey::new();

// The operation body contains application data
let body = Body::new("Hello, Sloth!".as_bytes());

// Custom extensions can be attached to an operation
type CustomExtensions = ();

// Create a header
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
}.sign(&private_key);

// Verify the header follows all protocol requirements
assert!(header.verify());

// An operation containing the header hash (the operation id), the header 
// itself and an optional body
let operation = Operation {
    hash: header.hash(),
    header,
    body: Some(body),
};

// Validate the header and, when included, that the body matches the `payload_hash`
assert!(validate_operation(&operation).is_ok());

```

## `Extensions`

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
    fn extract(&self) -> &LogId {
        &self.log_id
    }
}

impl Extension<Expiry> for CustomExtensions {
    fn extract(&self) -> &Expiry {
        &self.expires
    }
}

// Create an concrete extension, this will be encoded on a `Header`
let extensions = CustomExtensions {
    log_id: LogId(0),
    expires: Expiry(0123456),
};

// Extract the required fields by their type
let log_id = Extension::<LogId>::extract(&extensions);
let expires = Extension::<Expiry>::extract(&extensions);
```