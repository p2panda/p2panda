// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Display;

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub enum Access<C = ()> {
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

impl<C> Access<C> {
    pub fn pull() -> Self {
        Self::Pull
    }

    pub fn read() -> Self {
        Self::Read
    }

    pub fn write() -> Self {
        Self::Write { conditions: None }
    }

    pub fn manage() -> Self {
        Self::Manage
    }

    pub fn is_pull(&self) -> bool {
        matches!(self, Access::Pull)
    }

    pub fn is_read(&self) -> bool {
        matches!(self, Access::Read)
    }

    pub fn is_write(&self) -> bool {
        matches!(self, Access::Write { .. })
    }

    pub fn is_manage(&self) -> bool {
        matches!(self, Access::Manage)
    }
}
