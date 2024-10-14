// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

use crate::Header;

#[derive(Error, Debug)]
pub enum PruneError {
    // @TODO
    #[error("operation version {0} is not supported, needs to be <= {1}")]
    UnsupportedVersion(u64, u64),
}

pub fn validate_prunable_backlink<E>(
    past_header: Option<&Header<E>>,
    header: &Header<E>,
) -> Result<(), PruneError>
where
    E: Clone + Serialize + DeserializeOwned,
{
    todo!();
}
