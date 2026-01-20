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

Discovery can be used to find nodes which share a common interest in a topic.
During this process, transport information is exchanged in order to aid in the
establishment of direct peer-to-peer connections. A topic in p2panda is a
secret, randomly-generated hash that plays a similar role to a shared symmetric
key. Topics usually represent identifiers or namespaces for data and documents
associated with a specific group of people (for example a text document, chat
group or image folder). For this reason, a topic should never be leaked to
people outside of the intended group, whether accidentally or purposefully.

Our discovery protocol implementation is designed to ensure that topics are
never leaked to unintended actors. Nodes will only ever exchange data when both
parties have proven their knowledege of the same topic. This mutual
acknowledgement is achieved using a secure multiparty cryptographic technique
known as Private Equality Testing (PET) or Private Set Intersection (PSI) which
prevents unrelated topics being leaked to other parties.

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
