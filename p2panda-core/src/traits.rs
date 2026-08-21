// SPDX-License-Identifier: MIT OR Apache-2.0tra

//! Traits expressing core features of peer-to-peer data types.
use std::fmt::{Debug, Display};
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

use crate::logs::SeqNum;
use crate::operation::{Body, PayloadSize};

/// Identifier of an operation author.
pub trait Author:
    Copy
    + Clone
    + Display
    + Debug
    + PartialEq
    + Eq
    + Ord
    + StdHash
    + Serialize
    + for<'de> Deserialize<'de>
{
}

/// Identifier of a single operation.
pub trait OperationId: Copy + Clone + Display + Debug + PartialEq + Eq + Ord + StdHash {}

#[cfg(any(test, feature = "test_utils"))]
impl OperationId for u32 {}
#[cfg(any(test, feature = "test_utils"))]
impl OperationId for &str {}

/// Returns (unique) hash digest, which can be used as identifier of this published data type.
pub trait Digest<ID>
where
    ID: OperationId,
{
    /// Hash digest of peer-to-peer data-type which can be used as the identifier.
    fn hash(&self) -> ID;
}

/// Returns the author of this published data type and a method to verify the authenticity of it.
pub trait Provenance<A>
where
    A: Author,
{
    /// Identity of the author of data-type.
    fn author(&self) -> A;

    /// Checks if data-type and given author is authentic.
    fn verify(&self) -> bool;
}

/// Hash-chain structure with integrity guarantees and sequence numbers as a performance
/// optimization.
pub trait Chain<ID> {
    /// Pointer at previous entry in log which gives us the integrity guarantee of the "hash chain".
    /// The first entry in a log returns `None`.
    fn backlink(&self) -> Option<ID>;

    /// Sequence numbers are helpful to fastly detect forks and use the much faster and optimized
    /// diffing strategy when the local log is not forked.
    fn seq_num(&self) -> SeqNum;
}

/// Additional data which can be removed from the on-chain data-type.
pub trait Offchain<ID> {
    /// Authenticated payload.
    ///
    /// Can be requested or removed independently from the peer-to-peer data-type (off-chain). Don't
    /// expect this to always be available.
    fn payload(&self) -> Option<&Body>;

    /// Hash digest of the payload.
    fn payload_hash(&self) -> Option<ID>;

    /// Size in bytes of the payload.
    fn payload_size(&self) -> PayloadSize;
}

/// Custom header extensions type.
///
/// User-defined extensions can be added to an operation's [`Header`](crate::Header) in order to
/// extend the basic functionality of the core p2panda data types or to encode application-specific
/// fields which should not be contained in the [`Body`].
///
/// This might be system-specific information relating to capabilities or key-agreement schemes
/// which is required to enforce access-control restrictions during sync. Alternatively, extensions
/// might be used to set expiration timestamps and deletion flags in order to facilitate garbage
/// collection of stale data from the network. The core p2panda data types intentionally don't
/// enforce a single approach to such areas where there are rightly many different approaches, with
/// the most suitable being dependent on specific use-case requirements.
///
/// Interfaces which use p2panda core data types can require certain extensions to be present on any
/// headers that their APIs accept using trait bounds. `p2panda-stream`, for example, uses the
/// [`PruneFlag`](crate::PruneFlag) in order to implement automatic network-wide garbage collection.
///
/// Extensions are encoded on a header and sent over the wire. We need to satisfy all trait
/// requirements that `Header` requires, including `Serialize` and `Deserialize`.
///
/// ## Example
///
/// ```
/// use p2panda_core::{Hash, Header, SigningKey};
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Clone, Debug, Serialize, Deserialize)]
/// struct LogId(Hash);
///
/// #[derive(Clone, Debug, Serialize, Deserialize)]
/// struct CustomExtensions {
///     log_id: Option<LogId>,
///     expires: u64,
/// }
///
/// let extensions = CustomExtensions {
///     log_id: None,
///     expires: 1787246796,
/// };
///
/// let signing_key = SigningKey::generate();
///
/// let header = Header::builder()
///     .body("Hello, Sloth".as_bytes())
///     .build(&signing_key, extensions.clone());
///
/// assert_eq!(header.extensions.expires, 1787246796);
/// ```
pub trait Extensions: Clone + Debug + for<'de> Deserialize<'de> + Serialize {}

impl<T> Extensions for T where T: Clone + Debug + for<'de> Deserialize<'de> + Serialize {}

/// Returns a displayable string representing the underlying value in a short format, easy to read
/// during debugging and logging.
pub trait ShortFormat {
    fn fmt_short(&self) -> String;
}
