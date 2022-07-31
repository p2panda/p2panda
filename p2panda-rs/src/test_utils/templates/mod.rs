// SPDX-License-Identifier: AGPL-3.0-or-later

//! Fixture templates which can be used to run a single test agains collections a cases.

mod entry_templates;
mod operation_templates;
mod version_fixture_templates;

pub use entry_templates::legacy_many_valid_entries;
pub use operation_templates::{
    legacy_implements_as_operation, legacy_many_valid_operations, legacy_many_verified_operations,
};
pub use version_fixture_templates::legacy_version_fixtures;
