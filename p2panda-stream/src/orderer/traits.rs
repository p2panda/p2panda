// SPDX-License-Identifier: MIT OR Apache-2.0

/// Data-type declaring "dependencies" which need to be processed _before_ this item can be
/// processed as well.
///
/// This extends it to become a Directed-Acyclic-Graph (DAG) where nodes represent the data-types
/// and the edges represent the "dependencies" to others. The DAG can be now used to reason about
/// what messages need to be processed before we can process this messsage and give us causal /
/// partial ordering.
pub trait Ordering<ID> {
    fn dependencies(&self) -> &[ID];
}
