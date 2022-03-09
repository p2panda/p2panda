// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for CDDL validation.
#[derive(Debug, Error)]
pub enum CddlValidationError {
    /// Errors coming from `cddl_cat` crate.
    #[error(transparent)]
    Validation(#[from] cddl_cat::ValidateError),
}
