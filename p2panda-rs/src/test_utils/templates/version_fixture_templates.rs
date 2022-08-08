// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest_reuse::template;

/// Template which will contain many version fixtures in the future.
#[template]
#[export]
#[rstest]
#[case::latest($crate::test_utils::fixtures::latest_fixture())]
fn version_fixtures(#[case] fixture: Fixture) {}

pub use version_fixtures;
