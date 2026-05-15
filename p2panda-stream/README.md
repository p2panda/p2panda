<h1 align="center">p2panda-stream</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Event stream processing for p2p data-types</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://docs.rs/p2panda-stream">
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

Interfaces to implement and compose event processors on top of data streams
with some p2panda implementations out-of-the-box, wrapping existing p2panda
crates for causal message ordering, log validation, access control and group
encryption CRDTs which might come in handy for some peer-to-peer applications.

See our [Documentation](https://docs.rs/p2panda-stream) for an overview of
available processors and interfaces to write your own.

> 🚧 This library is under active development and the APIs are not yet
> considered stable for production use. Core data types and user-facing APIs
> may still undergo breaking changes. Stability guarantees will improve with
> the release of v1.0.0.

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
