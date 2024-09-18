// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

#[derive(Error, Debug)]
pub enum EngineError {
    // @TODO: Can we do this without the ciborium error type?
    #[error("decoding operation header failed")]
    DecodingFailed(ciborium::de::Error<std::io::Error>),
}
