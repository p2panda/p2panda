<h1 align="center">p2panda-discovery</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Confidential topic and node discovery protocol</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://docs.rs/p2panda-discovery">
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

Discovery can be used to find nodes and their transport information (to aid establishing a direct peer-to-peer connection) which are interested in the same "topic". A topic in p2panda is a secret, randomly generated hash, similar to a shared symmetric key. Since topics usually represent identifiers or namespaces for data and documents for only a certain amount of people (for example a "text document" or "chat group" or "image folder") it should only be shared with exactly these people and never accidentially leaked in our protocols.

With this discovery protocol implementation we are introducing a concrete solution which allows nodes to only ever exchange data when both parties have proven that they are aware of the same topic. No other, unrelated topics will be "leaked" to any party. This is made possible using "Private Equality Testing" (PET) or "Private Set Intersection" which is a secure multiparty computation cryptographic technique.

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
