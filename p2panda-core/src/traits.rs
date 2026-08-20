// SPDX-License-Identifier: MIT OR Apache-2.0tra

use std::fmt::Debug;
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

use crate::logs::SeqNum;
use crate::operation::{Body, PayloadSize};

/// Identifier of an operation author.
pub trait Author:
    Copy + Clone + Debug + PartialEq + Eq + Ord + StdHash + Serialize + for<'de> Deserialize<'de>
{
}

/// Identifier of a single operation.
pub trait OperationId: Copy + Clone + Debug + PartialEq + Eq + Ord + StdHash {}

#[cfg(any(test, feature = "test_utils"))]
impl OperationId for u32 {}
#[cfg(any(test, feature = "test_utils"))]
impl OperationId for &str {}

/// Returns (unique) hash digest, which can be used as identifier of this published data type.
pub trait Digest<ID>
where
    ID: OperationId,
{
    fn hash(&self) -> ID;
}

/// Returns the author of this published data type and a method to verify the authenticity of it.
pub trait Provenance<A>
where
    A: Author,
{
    fn author(&self) -> A;

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
    fn payload(&self) -> Option<&Body>;

    fn payload_hash(&self) -> Option<ID>;

    fn payload_size(&self) -> PayloadSize;
}

/// Super-trait defining trait bounds required by custom extensions types.
pub trait Extensions: Clone + Debug + for<'de> Deserialize<'de> + Serialize {}

impl<T> Extensions for T where T: Clone + Debug + for<'de> Deserialize<'de> + Serialize {}
