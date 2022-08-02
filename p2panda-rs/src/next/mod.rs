// SPDX-License-Identifier: AGPL-3.0-or-later

//! Temporary module which will guide us through this massive refactoring.
pub mod document;
pub mod entry;
pub mod graph;
pub mod hash;
pub mod identity;
pub mod operation;
pub mod schema;
pub mod secret_group;
pub mod serde;
#[cfg(any(feature = "testing", test))]
pub mod test_utils;
#[cfg(target_arch = "wasm32")]
pub mod wasm;
