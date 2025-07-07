<h1 align="center">p2panda-encryption</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Decentralised data- and message encryption for groups</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://docs.rs/p2panda-encryption">
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

`p2panda-encryption` provides decentralised, secure data- and message encryption for groups with post-compromise security and optional forward secrecy.

The crate implements two different group key-agreement and encryption schemes for a whole range of use cases for applications which can't rely on a stable network connection or centralised coordination.

More detail about the particular implementation and design choices of `p2panda-encryption` can be found in our [in-depth blog post](https://p2panda.org/2025/02/24/group-encryption.html).

## Features

### Strong Security Guarantees

Key agreement in `p2panda-encryption` provides strong forward secrecy, while the security of the data itself depends on the encryption scheme used. The crate offers two different encryption schemes:

**Data Encryption** re-uses the same symmetric key known to the peer until a new key is introduced to the group, for example on member removal. Keys will stay in the network to allow decrypting older data, unless they are manually removed by the application for optional forward secrecy.

For **Message Encryption** peers agree on a secret to establish a [message ratchet](https://en.wikipedia.org/wiki/Double_Ratchet_Algorithm) per member, deriving a new key for each message sent. After decrypting the ciphertext of the message the key gets dropped for forward secrecy.

### Framework-Independent

This implementation is compatible with any data type, encoding format or transport, made for p2p applications which do not rely on constant internet connectivity.

Similar to our other p2panda crates, we aim to make our implementation "framework independent" while providing optional "glue code" to integrate it in into the larger [p2panda ecosystem](https://p2panda.org).

We're currently working on a high-level, easy-to-use integration layer which combines `p2panda-auth` and `p2panda-encryption` into a feature-complete and tested solution with authenticated roles and group management, nested groups, multi-device support, atomic transactions, message ordering and validation.

### Robustness in Decentralised Systems

`p2panda-encryption` has been specifically designed to be robust when used in decentralised systems. It accounts for use in scenarios without guaranteed connectivity between members of the group and corner cases where group changes (adding, removing members etc.) take place concurrently. No centralised server is required for coordination of the group.

The code in this crate is expressed as [pure functions](https://en.wikipedia.org/wiki/Pure_function) where state is passed around until it is finally "committed" into a persistence layer inside an atomic transaction. This allows fault-resilient writes to any database and makes applications robust against corruption of their state when crashes occur.

## Design

### Encryption Schemes

The first scheme we simply call **"Data Encryption"**, allowing peers to encrypt any data with a secret, symmetric key for a group. This will be useful for building applications where users who enter a group late will still have access to previously-created content, for example knowledge databases, wiki applications or a booking tool for rehearsal rooms.

A member will not learn about any newly-created data they are removed from the group since the key gets rotated on member removal. This should accommodate for many use-cases in p2p applications which rely on basic group encryption with post-compromise security (PCS) and forward secrecy (FS) during key agreement. Applications can optionally choose to remove encryption keys for forward secrecy if they so desire.

The second scheme is **"Message Encryption"**, offering a forward secure (FS) messaging ratchet, similar to Signal's [Double Ratchet algorithm](https://en.wikipedia.org/wiki/Double_Ratchet_Algorithm). Since secret keys are always generated for each message, a user can not easily learn about previously-created messages when getting hold of such key. We believe that the latter scheme will be used in more specialised applications, for example p2p group chats, as strong forward secrecy comes with it's own UX requirements, but we are excited to offer a solution for both worlds, depending on the application's needs.

### Key Bundles and Pre-Keys

Key bundles are published into the network by peers. These bundles include identity- and pre-keys which can be used by other peers to invite them into an encrypted group.

Pre-keys are used during the initial [X3DG](https://signal.org/docs/specifications/x3dh/) key agreement between two peers and can be limited to a single use or for a specified lifetime for forward secrecy.

### Secure Key-Agreement

To encrypt any data towards a group we need to first securely and efficiently make all members of the group aware of the secret key which will be used to encrypt the message. This takes place inside a key agreement protocol.

Both encryption schemes use the Two-Party Secure Messaging (2SM) Key Agreement Protocol as specified in the paper ["Key Agreement for Decentralized Secure Group Messaging with Strong Security Guarantees"](https://eprint.iacr.org/2020/1281.pdf>) (2020).

During the initial 2SM "round" (via X3DH) the forward secrecy is defined by the lifetime of the used pre-keys. For strong security guarantees it is recommended to use one-time pre-keys. If this requirement can be relaxed it is possible to use long-term pre-keys, with a lifetime defined by the application.

Each subsequent 2SM round (via HPKE) uses exactly one secret key, which is then dropped and replaced by a newly-generated key-pair. This gives the key-agreement protocol strong forward secrecy guarantees for each round, independent of the initially used pre-keys.

2SM is optimised to allow a group to learn about a new group secret (for example, after a member removal or group compromise) in `O(n)` steps where `n` is the number of members.

## Usage & Integration

There are various ways to use `p2panda-encryption`. We're currently working on a p2panda crate which gives a tested end-to-end solution for building secure, decentralised applications with p2panda data types. If you're interested in group encryption, roles and members management for your application but not building the "p2p backend", this is for you.

The second option comes with more flexibility if you're interested in integrating group encryption into your custom p2p data-types and algorithms but also requires more care around message ordering, group management, validation and authentication. We've tried to reduce the API surface for integrations into custom applications as much as possible. If you still struggle, please [reach out](https://p2panda.org/#contact) to us.

## Security

End-to-end encryption (E2EE) solutions like `p2panda-encryption` prevent third parties from reading your application data but they can never guarantee full security, especially in decentralised, experimental networks.

We currently _cannot_ recommend using this technology for high-risk use-cases when you cannot fully guarantee control over all devices and transport channels.

### Audit

This crate has not yet received a security audit.

### Meta-Data

In the current implementation all group control messages are _unencrypted_. While application data is fully protected, an adversary who gains access to the network will be able to observe control messages and reason about which members are inside the group. The cryptographic identities in the group are not necessarily connected to any concrete persons but can potentially reveal enough meta-data to prove harmful.

We're working on a variant of `p2panda-encryption` where even control messages, sender and recipient info are encrypted. This unfortunately comes with worse performance and special UX requirements but we still believe there is a use-case for smaller groups.

### Post-Quantum

While a future of post-quantum computers may seem far away, `p2panda-encryption` is not secure against so called harvest-now-decrypt-later (HNDL) quantum adversaries as we're not using any post-quantum-ready cryptography.

## Credits

We have been particularly inspired by the ["Key Agreement for Decentralized Secure Group Messaging with Strong Security Guarantees"](https://eprint.iacr.org/2020/1281.pdf) (DCGKA) paper by Matthew Weidner, Martin Kleppmann, Daniel Hugenroth and Alastair R. Beresford (published in 2020) which is the first paper we are aware of which introduces a PCS and FS encryption scheme with a local-first mindset. On top there's already an almost-complete [Java implementation](https://github.com/trvedata/key-agreement) of the paper, which helped with realising our Rust version.

The paper formed the initial starting point of our work. In particular, we followed the Double-Ratchet "Message Encryption" scheme with some improvements around managing group membership. We also carried over some of the ideas in the paper to accommodate for the simpler "Data Encryption" approach.

Our implementation uses Signal's [X3DH](https://signal.org/docs/specifications/x3dh) key-agreement for initial rounds. This includes Signal's work around the [XEdDSA](https://signal.org/docs/specifications/xeddsa) signature schemes.

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
