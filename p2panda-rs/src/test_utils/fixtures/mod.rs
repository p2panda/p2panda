
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `rstest` fixtures and templates which can be injected into tests
//!
//! From the `rstest` docs: "rstest uses procedural macros to help you on writing fixtures and table-based tests.
//! The core idea is that you can inject your test dependencies by passing them as test arguments."
//!
//! With templates you can apply many rstest cases to a single test. They utilize the somewhat experimental
//! [rstest_reuse](https://github.com/la10736/rstest/tree/master/rstest_reuse) crate.
//!
//! <https://github.com/la10736/rstest>
//! 
//! See `fixtures/tests.rs` for examples. 

#[cfg(test)]
pub mod templates;
#[cfg(test)]
pub mod defaults;
#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub use fixtures::*;
