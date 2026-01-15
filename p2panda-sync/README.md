<h1 align="center">p2panda-sync</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Data-type agnostic traits for p2p sync with implementations</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://docs.rs/p2panda-sync">
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

Data-type agnostic interfaces for implementing sync protocols and managers which can be used
stand-alone or as part of the local-first stack provided by `p2panda-net`.

Users can implement two-party sync protocols over a `Sink` / `Stream` pair with the `Protocol`
trait and a system for instantiating and orchestrating concurrent sync sessions with the `Manager`
trait. 

Concrete implementations for performing sync over p2panda append-only logs associated with a
generic topic can be found in the `manager` and `protocols` modules.

For most high-level users `p2panda-net` will be the entry point into local-first development with
p2panda. Interfaces in this crate are intended for cases where users want to integrate their own
base convergent data-type and sync protocols as a module in the `p2panda-net` stack.

## License

Licensed under either of [Apache License, Version 2.0] or [MIT license] at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
p2panda by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

[Apache License, Version 2.0]: https://github.com/p2panda/p2panda/blob/main/LICENSES/Apache-2.0.txt
[MIT license]: https://github.com/p2panda/p2panda/blob/main/LICENSES/MIT.txt
[p2panda-net]: https://docs.rs/p2panda-net/latest/p2panda_net/

---

_This project has received funding from the European Union’s Horizon 2020
research and innovation programme within the framework of the NGI-POINTER
Project funded under grant agreement No 871528, NGI-ASSURE No 957073 and
NGI0-ENTRUST No 101069594_.