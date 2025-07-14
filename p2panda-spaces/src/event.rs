// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::group::Group;
use crate::space::Space;

pub enum Event<F, M> {
    JoinedSpace(Space<F, M>),
    JoinedGroup(Group),
    Message(M),
}
