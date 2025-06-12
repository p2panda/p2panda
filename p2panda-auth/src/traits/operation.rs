// SPDX-License-Identifier: MIT OR Apache-2.0

/// Interface to express required information from operations processed by any auth graph
/// implementation.
///
/// Applications implementing these traits should authenticate the original sender of each
/// operation.
pub trait Operation<ID, OP, P> {
    /// Id of this operation.
    fn id(&self) -> OP;

    /// ID of the author of this operation.
    fn author(&self) -> ID;

    /// Other operation dependencies.
    fn dependencies(&self) -> Vec<OP>;

    /// The last graph tips in the group this operation belongs to.
    fn previous(&self) -> Vec<OP>;

    /// Payload of this operation.
    fn payload(&self) -> P;
}
