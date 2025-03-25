// SPDX-License-Identifier: MIT OR Apache-2.0

use thiserror::Error;

use crate::crypto::RngError;
use crate::crypto::xeddsa::XEdDSAError;

#[derive(Debug, Error)]
pub enum X3DHError {
    #[error(transparent)]
    Rng(#[from] RngError),

    #[error(transparent)]
    XEdDSA(#[from] XEdDSAError),
}
