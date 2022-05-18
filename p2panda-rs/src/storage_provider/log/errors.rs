// SPDX-License-Identifier: AGPL-3.0-or-later

/// `LogStorage` errors.
#[derive(thiserror::Error, Debug)]
pub enum LogStorageError {
    /// Catch all error which implementers can use for passing their own errors up the chain.
    #[error("Error occured during `LogStorage` request in storage provider: {0}")]
    Custom(String),
}
