// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Display;

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub enum Access<C> {
    Pull,
    Read,
    Write { conditions: Option<C> },
    Manage,
}

impl<C> Display for Access<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Access::Pull => "pull",
            Access::Read => "read",
            Access::Write { .. } => "write",
            Access::Manage => "manage",
        };

        write!(f, "{}", s)
    }
}
