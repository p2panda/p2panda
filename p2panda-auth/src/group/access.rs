// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Display;

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub enum AccessLevel {
    Pull,
    Read,
    Write,
    Manage,
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub struct Access<C = ()> {
    pub level: AccessLevel,
    pub conditions: Option<C>,
}

impl<C> Display for Access<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self.level {
            AccessLevel::Pull => "pull",
            AccessLevel::Read => "read",
            AccessLevel::Write => "write",
            AccessLevel::Manage => "manage",
        };

        write!(f, "{}", s)
    }
}

impl<C> Access<C> {
    pub fn pull() -> Self {
        Self {
            level: AccessLevel::Pull,
            conditions: None,
        }
    }

    pub fn read() -> Self {
        Self {
            level: AccessLevel::Read,
            conditions: None,
        }
    }

    pub fn write() -> Self {
        Self {
            level: AccessLevel::Write,
            conditions: None,
        }
    }

    pub fn manage() -> Self {
        Self {
            level: AccessLevel::Manage,
            conditions: None,
        }
    }

    pub fn with_conditions(mut self, conditions: C) -> Self {
        self.conditions = Some(conditions);
        self
    }

    pub fn is_pull(&self) -> bool {
        matches!(self.level, AccessLevel::Pull)
    }

    pub fn is_read(&self) -> bool {
        matches!(self.level, AccessLevel::Read)
    }

    pub fn is_write(&self) -> bool {
        matches!(self.level, AccessLevel::Write)
    }

    pub fn is_manage(&self) -> bool {
        matches!(self.level, AccessLevel::Manage)
    }
}
