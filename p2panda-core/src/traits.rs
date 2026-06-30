// SPDX-License-Identifier: MIT OR Apache-2.0

use std::hash::Hash as StdHash;

use crate::identity::Author;

/// Identifier of a single operation.
pub trait OperationId: Copy + Clone + PartialEq + Eq + StdHash {}

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

/// Returns a displayable string representing the underlying value in a short format, easy to read
/// during debugging and logging.
pub trait ShortFormat {
    fn fmt_short(&self) -> String;
}
