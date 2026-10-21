// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::{Display, Formatter, Result};

/// All different stream types.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StreamType {
    Topic,
    Group,
    Space,
}

impl Display for StreamType {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let name = match self {
            Self::Topic => "topic",
            Self::Group => "group",
            Self::Space => "space",
        };

        write!(f, "{name}-stream")
    }
}
