// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SchemaIdError {
    #[error("invalid hash string")]
    HashError(#[from] crate::hash::HashError),
}
