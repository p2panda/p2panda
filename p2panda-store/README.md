<h1 align="center">p2panda-store</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Store traits and implementations</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://docs.rs/p2panda-store">
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

Interfaces and implementations of persistence layers for p2panda data types and application
states.

p2panda follows a strict separation of read- and write-only database interfaces to allow designing efficient and fail-safe [atomic transactions](https://youtu.be/5ZjhNTM8XU8?feature=shared&t=420) throughout the stack.

## Read queries

`p2panda-store` currently offers all read-only trait interfaces for commonly used p2panda core data-types and flows (for example "get the latest operation for this log"). These persistence and query APIs are utilised by higher-level components of the p2panda stack, such as `p2panda-sync` and `p2panda-stream`. For detailed information concerning the `Operation` type, please consult the documentation for the `p2panda-core` crate.

## Write transactions

Multiple writes to a database should be grouped into one single, atomic transaction when they need to strictly _all_ occur or _none_ occur. This is crucial to guarantee a crash-resiliant p2p application, as any form of failure and disruption (user moving mobile app into the background, etc.) might otherwise result in invalid database state which is hard to recover from.

`p2panda-store` offers `WritableStore`, `Transaction` and `WriteToStore` traits to accommodate for exactly such a system and all p2panda implementations strictly follow the same pattern.

```rust,ignore
// Initialise a concrete store implementation, for example for SQLite. This implementation
// needs to implement the `WritableStore` trait, providing it's native transaction interface.

let mut store = Sqlite::new();

// Establish state, do things with it. `User` and `Event` both implement `WriteToStore` for the
// concrete store type `Sqlite`.

let user = User::new("casey");
let mut event = Event::new("Ants Research Meetup");
event.register_attendance(&user);

// Persist state in database in one single, atomic transaction.

let mut tx = store.begin().await?;

user.write(&mut tx).await?;
event.write(&mut tx).await?;

tx.commit().await?;
```

It is recommended for application developers to re-use similar transaction patterns to leverage the same crash-resiliance guarantees for their application-layer state and persistance handling.

## Store implementations

Read queries and atomic write transactions are implemented for all p2panda-stack related data types for concrete databases: Memory and SQLite.

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
