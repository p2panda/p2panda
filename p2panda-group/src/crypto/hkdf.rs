// SPDX-License-Identifier: MIT OR Apache-2.0

//! Hashed Message Authentication Code (HMAC)-based key derivation function (HKDF) using
//! "hash-mode" with SHA256.
//!
//! <https://www.rfc-editor.org/rfc/rfc5869>
use libcrux_hkdf::Algorithm;
use thiserror::Error;

const HKDF: Algorithm = Algorithm::Sha256;

pub fn hkdf<const N: usize>(
    salt: &[u8],
    ikm: &[u8],
    info: Option<&[u8]>,
) -> Result<[u8; N], HkdfError> {
    let hash = libcrux_hkdf::hkdf(HKDF, salt, ikm, info.unwrap_or_default(), N)
        .map_err(|_| HkdfError::InvalidArguments)?;
    Ok(hash
        .try_into()
        .expect("matching output size from hkdf function"))
}

#[derive(Debug, Error)]
pub enum HkdfError {
    #[error("arguments too large for hkdf")]
    InvalidArguments,
}

#[cfg(test)]
mod tests {
    use super::hkdf;

    #[test]
    fn key_material_len() {
        let result_1: [u8; 18] = hkdf(b"salt", b"ikm", None).unwrap();
        assert_eq!(result_1.len(), 18);
    }

    #[test]
    fn info_needs_to_match() {
        let result_1: [u8; 18] = hkdf(b"salt", b"ikm", Some(b"info")).unwrap();
        let result_2: [u8; 18] = hkdf(b"salt", b"ikm", Some(b"info")).unwrap();
        let result_3: [u8; 18] = hkdf(b"salt", b"ikm", Some(b"different info")).unwrap();
        assert_eq!(result_1, result_2);
        assert_ne!(result_2, result_3);
    }
}
