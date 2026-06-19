// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(test)]
mod tests;
mod traits;

pub use sqlite::SqliteSpacesStore;
pub use traits::{SpacesMessageStore, SpacesStore, SpacesStoreWrite};

use p2panda_core::{Hash, VerifyingKey};

/// Spaces message type with generic parameter for additional arguments.
// @TODO: this type is so generic that it could actually be used / defined somewhere else. It
// represents a message with id and author, replacing the need for trait interfaces.
#[derive(Debug, Clone)]
pub struct SpacesMessage<T> {
    pub id: Hash,
    pub author: VerifyingKey,
    pub args: T,
}
