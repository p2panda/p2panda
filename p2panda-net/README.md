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
