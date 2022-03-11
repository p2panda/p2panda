// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

#[allow(missing_copy_implementations)]
#[derive(Error, Debug, Clone, PartialEq)]
pub enum MemoryStoreError {
    #[error("not a very useful error!")]
    Error,
}
