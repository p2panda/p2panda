<h1 align="center">p2panda-net</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Data-type-agnostic p2p networking</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://docs.rs/p2panda-net">
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

This crate provides a data-type-agnostic p2p networking layer offering robust, direct communication
to any device, no matter where they are.

It provides a stream-based API for higher layers: Applications subscribe to any "topic" they are
interested in and `p2panda-net` will automatically discover similar peers and transport raw bytes
between them.

Additionally `p2panda-net` can be extended with custom sync protocols for all data types, allowing
applications to "catch up on past data", eventually converging to the same state.

Most of the lower-level networking of `p2panda-net` is made possible by the work of
[iroh](https://github.com/n0-computer/iroh/) utilising well-established and known standards, like
QUIC for transport, (self-certified) TLS for transport encryption, STUN for establishing direct
connections between devices, Tailscale's DERP (Designated Encrypted Relay for Packets) for relay
fallbacks, PlumTree and HyParView for broadcast-based gossip overlays.

## Features

- Data of any kind can be exchanged efficiently via gossip broadcast ("live mode") or via sync
  protocols between two peers ("catching up on past state")
- Custom network-wide queries to express interest in certain data of applications
- Ambient peer discovery: Learning about new, previously unknown peers in the network
- Ambient topic discovery: Learning what peers are interested in, automatically forming
  overlay networks per topic
- Sync protocol API, providing an eventual-consistency guarantee that peers will converge on
  the same state over time
- Manages connections, automatically syncs with discovered peers and re-tries on faults
- Extension to handle efficient sync of large files

## Example

```rust
use anyhow::Result;
use p2panda_core::{PrivateKey, Hash};
use p2panda_discovery::mdns::LocalDiscovery;
use p2panda_net::{NetworkBuilder, TopicId};
use p2panda_sync::TopicQuery;
use serde::{Serialize, Deserialize};

// The network can be used to automatically find and ask other peers about any data the
// application is interested in. This is expressed through "network-wide queries" over topics.
//
// In this example we would like to be able to query messages from each chat group, identified
// by a BLAKE3 hash.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
struct ChatGroup(Hash);

impl ChatGroup {
    pub fn new(name: &str) -> Self {
        Self(Hash::new(name.as_bytes()))
    }
}

impl TopicQuery for ChatGroup {}

impl TopicId for ChatGroup {
    fn id(&self) -> [u8; 32] {
        self.0.into()
    }
}

async fn run() -> Result<()> {
    // Peers using the same "network id" will eventually find each other. This
    // is the most global identifier to group peers into multiple networks when
    // necessary.
    let network_id = [1; 32];

    // Generate an Ed25519 private key which will be used to authenticate your peer towards others.
    let private_key = PrivateKey::new();

    // Use mDNS to discover other peers on the local network.
    let mdns_discovery = LocalDiscovery::new();

    // Establish the p2p network which will automatically connect you to any discovered peers.
    let network = NetworkBuilder::new(network_id.into())
        .private_key(private_key)
        .discovery(mdns_discovery)
        .build()
        .await?;

    // From now on we can send and receive bytes to any peer interested in the same chat.
    let my_friends_group = ChatGroup::new("me-and-my-friends");
    let (tx, mut rx, ready) = network.subscribe(my_friends_group).await?;

    Ok(())
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

*This project has received funding from the European Union’s Horizon 2020
research and innovation programme within the framework of the NGI-POINTER
Project funded under grant agreement No 871528, NGI-ASSURE No 957073 and
NGI0-ENTRUST No 101069594*.
