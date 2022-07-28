// SPDX-License-Identifier: AGPL-3.0-or-later

//! Temporary module which will guide us through this massive refactoring.
// Please note: These modules need to be named with the verbose `_fixtures` suffix, otherwise
// `rstest` will get confused by methods with similar names.
mod document_fixtures;
mod operation_fixtures;
mod schema_fixtures;
mod version_fixtures;

pub use document_fixtures::*;
pub use operation_fixtures::*;
pub use schema_fixtures::*;
pub use version_fixtures::*;
