use std::fmt::Display;

use serde::{Deserialize, Serialize};

/// Access levels which can be assigned to a group member.
///
/// Access levels are ordered such that "higher" access levels include all "lower" onces.
///
/// Pull < Read < Write < Manage
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Ord, Serialize, Deserialize)]
pub enum Access {
    Pull,
    Read,
    Write,
    Manage, // Admin
}

impl Display for Access {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Access::Pull => "pull",
            Access::Read => "read",
            Access::Write => "write",
            Access::Manage => "manage",
        };

        write!(f, "{}", s)
    }
}