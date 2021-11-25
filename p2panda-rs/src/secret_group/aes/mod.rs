// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods to encrypt and decrypt data symmetrically with AES256 block cipher using GCM-SIV
//! operation mode.
//!
//! See: <https://www.imperialviolet.org/2017/05/14/aesgcmsiv.html>
mod aes;
mod error;

pub use aes::{decrypt, encrypt};
pub use error::AesError;
