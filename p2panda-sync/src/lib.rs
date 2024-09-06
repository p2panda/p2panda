// SPDX-License-Identifier: AGPL-3.0-or-later

#[cfg(feature = "core")]
mod codec;
#[cfg(feature = "core")]
pub mod engine;
#[cfg(feature = "log-height")]
pub mod protocols;
pub mod traits;

pub use engine::{Engine, Session};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("input/output error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("codec error: {0}")]
    Codec(String),
    #[error("custom error: {0}")]
    Custom(String),
}