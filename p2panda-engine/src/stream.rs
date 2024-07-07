// SPDX-License-Identifier: AGPL-3.0-or-later

use p2panda_core::{Extension, Operation};

#[derive(Debug)]
pub enum StreamEvent<E>
where
    E: Extension,
{
    Commit(Operation<E>),
    Replay(Vec<Operation<E>>),
}
