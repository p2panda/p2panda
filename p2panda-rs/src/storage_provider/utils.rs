// SPDX-License-Identifier: AGPL-3.0-or-later

//! Utility methods and types for the storage provider module.

/// A custom `Result` type to be able to dynamically propagate `Error` types.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Sync + Send>>;
