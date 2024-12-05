<h1 align="center">p2panda-stream</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Stream-based methods to conveniently handle p2panda operations</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://p2panda.org">
      Website
    </a>
  </h3>
</div>

This crate provides a collection of various methods which help to decode, validate, order,
prune or store p2panda operations. More methods are planned in the future.

With the stream-based design it is easy to "stack" these methods on top of each other,
depending on the requirements of the application (or each "topic" data stream). Like this a
user can decide if they want to persist data or keep it "ephemeral", apply automatic pruning
techniques for outdated operations etc.

## License

Licensed under either of

* Apache License, Version 2.0 ([Apache-2.0.txt](https://github.com/p2panda/p2panda/blob/main/LICENSES/Apache-2.0.txt) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([MIT.txt](https://github.com/p2panda/p2panda/blob/main/LICENSES/MIT.txt) or https://mit-license.org/)

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
p2panda by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
