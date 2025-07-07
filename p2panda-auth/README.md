<h1 align="center">p2panda-auth</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Decentralised group management with fine-grained, per-member permissions</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://docs.rs/p2panda-auth">
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

This crate provides decentralised group management with fine-grained, per-member
permissions.

Once a group has been created, members can be added, removed, promoted and demoted. Each
member has an associated access level which can be used to determine their permissions. The
access levels are `Pull`, `Read`, `Write` and `Manage`. Each access level is a superset of the
lower levels and can be assigned an associated set of conditions; this allows fine-grained
partitioning of each access level. For example, `Read` conditions could be assigned with a
path to restrict access to areas of a dataset. Finally, only members with `Manage` access are
allowed to modify the group state by adding, removing, promoting or demoting other members.

The access levels defined by `p2panda-auth` may prove useful when controlling replication of
datasets. Custom sync protocols can be defined which rely on group membership and access
levels to determine who to sync with and over which subsets of data. Access conditions can be
used to define application-layer specific access rules, for example when modelling moderation
rules or additional write checks.

## Features

### Eventually Consistent Group State

Peers replicating group operations will all eventually arrive at the same group state, even
when operations are authored concurrently or received out of order. Resolution of conflicting
state happens automatically.

### Strict Group Modification

Only operations authored by members with “manage” access level will be applied to the group
state.

### Customisable Concurrency Resolution

A group operation "resolver" is used to decide which operations should be invalidated in
certain concurrent situations. While the default concurrency resolver follows a cautious
"strong removal" approach, alternative approaches can be realised using custom implementations
of the provided `Resolver` trait.

## Design

### Group Operations

Group state is modified by the publication of group operations. Each operation is signed by
the author and includes an action, along with fields to define previous operations and other
dependencies. The previous field allows causal ordering of operations in relation to one
another and the dependencies field allows custom application logic to define relationships
between groups and group operations.

### Directed Acyclic Graph (DAG)

All operations comprising a group for a Directed Acyclic Graph (DAG). This data structure
allows for concurrent operations to published and later resolved by merging divergent states.

### Concurrency Resolution for Conflicting Operations

Certain concurrent scenarios lead to group state conflicts which must be resolved. In such
cases, all operations in the DAG are walked in a depth-first search so that any "bubbles" of
concurrent operations may be identified. Resolution rules are then applied to the operations
in these bubbles in order to populate a filter of operations to be invalidated. Once the
offending operations have been invalidated, any dependent operations are then invalidated in
turn.

The provided "strong removal" resolver defines the following rules:

1. Removal or demotion of a manager causes any concurrent actions by that member to be
   invalidated
2. Mutual removals, where two managers remove or demote one another concurrently, are not
   invalidated; both removals are applied to the group state but any other concurrent actions
   by those members are invalidated
3. Re-adds are allowed; if Alice removes Charlie then re-adds them, they are still a member of
   the group but all of their concurrent actions are invalidated
4. Invalidation of transitive operations; invalidation of an operation due to the application
   of the aforementioned rules results in all dependent operations being invalidated

## Related work

The TypeScript library [localfirst/auth](https://github.com/local-first-web/auth) uses groups to
define access-control and encryption boundaries. We were quite influenced by their flexible CRDT
abstraction which allows for users to plug in their own "resolvers".

The [Ink & Switch](https://www.inkandswitch.com) project
[keyhive](https://www.inkandswitch.com/keyhive/notebook/) also uses a groups abstraction for their
integration of access-control and encryption systems into the automerge CRDT library. The
underlying mechanisms for managing group state however is based on chains of capability
delegations, which is different from our operation-based CRDT approach.

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
