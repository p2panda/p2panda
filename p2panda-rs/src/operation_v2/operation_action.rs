// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;
use std::fmt::Display;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OperationAction {
    /// Operation creates a new document.
    Create,

    /// Operation updates an existing document.
    Update,

    /// Operation deletes an existing document.
    Delete,
}

impl OperationAction {
    /// Returns the operation action as a string.
    pub fn as_str(&self) -> &str {
        match self {
            OperationAction::Create => "create",
            OperationAction::Update => "update",
            OperationAction::Delete => "delete",
        }
    }
}

impl Display for OperationAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
