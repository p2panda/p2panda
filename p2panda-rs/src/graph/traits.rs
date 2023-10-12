// SPDX-License-Identifier: AGPL-3.0-or-later

use std::error::Error;
use std::fmt::Debug;

/// Trait to be defined on a reducer which can be passed into `Graph` in order to perform an
/// operation on all visited nodes.
///
/// It has one method `combine` which takes a generic value and mutable self.
pub trait Reducer<V> {
    /// Error type returned from `combine`
    type Error: Error + Debug;

    /// Takes a generic value and presumably combines it with some contained state.
    fn combine(&mut self, value: &V) -> Result<(), Self::Error>;
}
