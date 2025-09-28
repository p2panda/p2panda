// SPDX-License-Identifier: MIT OR Apache-2.0

use std::hash::Hash as StdHash;

pub trait OperationId: Copy + Clone + PartialEq + Eq + StdHash {}

pub trait Identifier<ID>
where
    ID: OperationId,
{
    fn id(&self) -> &ID;
}
