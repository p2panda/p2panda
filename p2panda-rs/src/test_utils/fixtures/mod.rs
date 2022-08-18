// SPDX-License-Identifier: AGPL-3.0-or-later

//! General purpose fixtures which can be injected into test methods as parameters.
//!
//! The fixtures can optionally be passed in with custom parameters which overrides the default
//! values. See examples for more details.
//!
//! Implemented using the [`rstest`](https://github.com/la10736/rstest) library.
mod document_fixtures;
mod entry_fixtures;
mod hash_fixtures;
mod identity_fixtures;
mod operation_fixtures;
mod schema_fixtures;
mod version_fixtures;

// These modules need to be named with the verbose `_fixtures` suffix, otherwise `rstest` will get
// confused by methods with similar names.
pub use document_fixtures::*;
pub use entry_fixtures::*;
pub use hash_fixtures::*;
pub use identity_fixtures::*;
pub use operation_fixtures::*;
pub use schema_fixtures::*;
pub use version_fixtures::*;
