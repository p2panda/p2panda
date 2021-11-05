// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for methods of `Materializer` struct.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum MaterializationError {
    /// Materialization error.
    #[error("materialization error")]
    Error,
}
