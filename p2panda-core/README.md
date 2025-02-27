<h1 align="center">p2panda-core</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Extensible, secure and distributed data-types</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://docs.rs/p2panda-core">
      Documentation
    </a>
    <span> | </span>
    <a href="https://github.com/p2panda/p2panda/releases">
      Releases
    </a>
    <span> | </span>
    <a href="https://p2panda.org">
      Website
    </a>
  </h3>
</div>

Highly extensible data-types of the p2panda protocol for secure, distributed and efficient
exchange of data, supporting networks from the internet to packet radio, LoRa or BLE. 

The primary data structure is an append-only implementation which supports history deletion,
multi-writer ordering, fork-tolerance, efficient partial sync, compatibility with any CRDT and is
extensible depending on your application requirements.

## Features

- Cryptographic signatures for authorship verification and tamper-proof messages
- Authors can maintain one or many logs
- Single-writer logs which can be combined to support multi-writer collaboration
- Compatible with any application data and CRDT
- Various ordering algorithms
- Supports efficient, partial sync
- Compatible with any networking scenario (even broadcast-only, for example for packet radio)
- Fork-tolerant
- Pruning of outdated messages
- Highly extensible with custom features, for example prefix-deletion, ephemeral
  "self-destructing" messages, etc.

## Examples

### Create and sign operation

```rust
use p2panda_core::{Body, Header, PrivateKey};

let private_key = PrivateKey::new();

let body = Body::new("Hello, Panda!".as_bytes());
let mut header = Header {
    version: 1,
    public_key: private_key.public_key(),
    signature: None,
    payload_size: body.size(),
    payload_hash: Some(body.hash()),
    timestamp: 1733170247,
    seq_num: 0,
    backlink: None,
    previous: vec![],
    extensions: None::<()>,
};

header.sign(&private_key);
```

### Custom extensions

Custom functionality can be added using extensions, for example, access-control
tokens, self-destructing messages, or encryption schemas.

```rust
use p2panda_core::{Extension, Header, PrivateKey};
use serde::{Serialize, Deserialize};

// Extend our operations with an "expiry" field we can use to implement
// "ephemeral messages" in our application, which get automatically deleted
// after the expiration timestamp is due.
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Expiry(u64);

// Multiple extensions can be combined in a custom type.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct CustomExtensions {
    expiry: Expiry,
}

// Implement `Extension<T>` for each extension we want to add to our
// header.
impl Extension<Expiry> for CustomExtensions {
    fn extract(header: &Header<Self>) -> Option<Expiry> {
        header
            .extensions
            .as_ref()
            .map(|extensions| extensions.expiry.clone())
    }
}
```

## License

Licensed under either of [Apache License, Version 2.0] or [MIT license] at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
p2panda by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

[Apache License, Version 2.0]: https://github.com/p2panda/p2panda/blob/main/LICENSES/Apache-2.0.txt
[MIT license]: https://github.com/p2panda/p2panda/blob/main/LICENSES/MIT.txt

---

_This project has received funding from the European Union’s Horizon 2020
research and innovation programme within the framework of the NGI-POINTER
Project funded under grant agreement No 871528, NGI-ASSURE No 957073 and
NGI0-ENTRUST No 101069594_.
