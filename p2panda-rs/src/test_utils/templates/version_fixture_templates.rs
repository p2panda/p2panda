// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest_reuse::template;

/// Template which will contain many version fixtures in the future.
#[template]
#[export]
#[rstest]
#[case::v0_3_0($crate::test_utils::fixtures::v0_3_0_fixture())]
fn legacy_version_fixtures(#[case] fixture: Fixture) {}

pub use legacy_version_fixtures;
