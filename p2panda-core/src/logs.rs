// SPDX-License-Identifier: MIT OR Apache-2.0

use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

/// Uniquely identify a single-author log.
///
/// The `LogId` exists purely to group a set of operations and is intended to be implemented for
/// any type which meets the design requirements of a particular application.
///
/// A blanket implementation is provided for any type meeting the required trait bounds.
///
/// Here we briefly outline several implementation scenarios:
///
/// An application relying on a one-log-per-author design might choose to implement `LogId` for a
/// thin wrapper around an Ed25519 public key; this effectively ties the log to the public key of
/// the author. Secure Scuttlebutt (SSB) is an example of a protocol which relies on this model.
///
/// In an application where one author may produce operations grouped into multiple logs, `LogId`
/// might be represented a unique number for each log instance.
///
/// Some applications might require semantic grouping of operations. For example, a chat
/// application may choose to create a separate log for each author-channel pairing. In such a
/// scenario, `LogId` might be implemented for a `struct` containing a `String` representation of
/// the channel name.
///
/// Finally, please note that implementers of `LogId` must take steps to ensure their log design
/// is fit for purpose and that all operations have been thoroughly validated before being
/// persisted. No such validation checks are provided by `p2panda-store`.
pub trait LogId: Clone + Eq + StdHash + Serialize + for<'de> Deserialize<'de> {}

impl<T> LogId for T where T: Clone + Eq + StdHash + Serialize + for<'de> Deserialize<'de> {}
