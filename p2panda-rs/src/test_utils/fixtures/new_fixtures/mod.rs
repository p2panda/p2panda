// SPDX-License-Identifier: AGPL-3.0-or-later

//! General purpose fixtures which can be injected into rstest methods as parameters.
//!
//! The fixtures can optionally be passed in with custom parameters which overrides the default
//! values.

mod document_fixtures;
mod entry_fixtures;
mod hash_fixtures;
mod identity_fixtures;
mod operation_fixtures;
mod schema_fixtures;

pub use document_fixtures::*;
pub use entry_fixtures::*;
pub use hash_fixtures::*;
pub use identity_fixtures::*;
pub use operation_fixtures::*;
pub use schema_fixtures::*;
