// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod protocols;
pub mod traits;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SyncError {
    /// Error which can occur in a running sync session
    #[error("sync protocol error: {0}")]
    Protocol(String),

    /// I/O error which occurs during stream handling 
    #[error("input/output error: {0}")]
    IoError(#[from] std::io::Error),

    /// Error which occurs when encoding or decoding protocol messages
    #[error("codec error: {0}")]
    Codec(String),

    /// Custom error to handle other cases
    #[error("custom error: {0}")]
    Custom(String),
}

pub type TopicId = [u8; 32];

#[derive(PartialEq, Debug)]
pub enum FromSync {
    Topic(TopicId),
    Bytes(Vec<u8>),
}
