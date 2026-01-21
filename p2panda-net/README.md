<h1 align="center">p2panda-net</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Data-type-agnostic p2p networking, discovery, gossip and local-first sync</strong>
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

`p2panda-net` is a collection of Rust modules providing solutions for a whole
set of peer-to-peer and [local-first] application requirements. Collectively
these modules solve the problem of event delivery.

Applications subscribe to any topic they are interested in and `p2panda-net`
will automatically discover similar peers and exchange messages between them.

> [!IMPORTANT]
> `p2panda-net` depends on a fixed version
> ([`#117`](https://github.com/n0-computer/iroh-gossip/pull/117)) of
> `iroh-gossip` which has not been published yet.
>
> Please patch your build with the fixed crate for now until this is handled
> upstream by adding the following lines to your root `Cargo.toml`:
>
> ```toml
> [patch.crates-io]
> iroh-gossip = { git = "https://github.com/p2panda/iroh-gossip", rev = "533c34a2758518ece19c1de9f21bc40d61f9b5a5" }
> ```

## Features

- [Publish & Subscribe] for ephemeral messages (gossip protocol)
- Publish & Subscribe for messages with [Eventual Consistency] guarantee (sync
  protocol)
- Confidentially discover nodes who are interested in the same topic ([Private
  Set Intersection])
- Establish and manage direct connections to any device over the Internet
  (using [iroh])
- Monitor system with supervisors and restart modules on critical failure
  (Erlang-inspired [Supervision Trees])
- Modular API allowing users to choose or replace the layers they want to use

## Getting Started

Install the Rust crate using `cargo add p2panda-net`.

```rust
use futures_util::StreamExt;
use p2panda_core::Hash;
use p2panda_net::iroh_mdns::MdnsDiscoveryMode;
use p2panda_net::{AddressBook, Discovery, Endpoint, MdnsDiscovery, Gossip};

// Topics are used to discover other nodes and establish connections around them.
let topic = Hash::new(b"shirokuma-cafe").into();

// Maintain an address book of newly discovered or manually added nodes.
let address_book = AddressBook::builder().spawn().await?;

// Establish direct connections to any device with the help of iroh.
let endpoint = Endpoint::builder(address_book.clone())
    .spawn()
    .await?;

// Discover nodes on your local-area network.
let mdns = MdnsDiscovery::builder(address_book.clone(), endpoint.clone())
    .mode(MdnsDiscoveryMode::Active)
    .spawn()
    .await?;

// Confidentially discover nodes interested in the same topic.
let discovery = Discovery::builder(address_book.clone(), endpoint.clone())
    .spawn()
    .await?;

// Disseminate messages among nodes.
let gossip = Gossip::builder(address_book.clone(), endpoint.clone())
    .spawn()
    .await?;

// Join topic to publish and subscribe to stream of (ephemeral) messages.
let cafe = gossip.stream(topic).await?;

// This message will be seen by other nodes if they're online. If you want messages to arrive
// eventually, even when they've been offline, you need to use p2panda's "sync" module.
cafe.publish(b"Hello, Panda!").await?;

let mut rx = cafe.subscribe();
tokio::spawn(async move {
    while let Some(Ok(bytes)) = rx.next().await {
        println!("{}", String::from_utf8(bytes).expect("valid UTF-8 string"));
    }
});
```

For a complete command-line application using `p2panda-net` with a sync
protocol, see our [`chat.rs`] example.

[local-first]: https://www.inkandswitch.com/local-first-software/
[Publish & Subscribe]: https://en.wikipedia.org/wiki/Publish%E2%80%93subscribe_pattern
[Eventual Consistency]: https://en.wikipedia.org/wiki/Eventual_consistency
[Actor Model]: https://en.wikipedia.org/wiki/Actor_model
[Private Set Intersection]: https://en.wikipedia.org/wiki/Private_set_intersection
[Supervision Trees]: https://adoptingerlang.org/docs/development/supervision_trees/
[iroh]: https://www.iroh.computer/
[`chat.rs`]: https://github.com/p2panda/p2panda/blob/main/p2panda-net/examples/chat.rs

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
