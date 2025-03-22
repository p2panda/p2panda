// SPDX-License-Identifier: MIT OR Apache-2.0

//! XEdDSA enables use of a single key pair format for both x25519 elliptic curve Diffie-Hellman
//! and Ed25199 signatures.
//!
//! <https://signal.org/docs/specifications/xeddsa/>
use std::fmt;

use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use curve25519_dalek::{EdwardsPoint, MontgomeryPoint, Scalar};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use thiserror::Error;

use crate::crypto::ed25519::SIGNATURE_SIZE;
use crate::crypto::sha2::sha2_512;
use crate::crypto::x25519::{PublicKey, SecretKey};
use crate::traits::RandProvider;

const HASH_1_PREFIX: [u8; 32] = [
    0xFEu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8,
    0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8,
    0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8,
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct XSignature(#[serde(with = "serde_bytes")] [u8; SIGNATURE_SIZE]);

impl XSignature {
    pub fn from_bytes(bytes: [u8; SIGNATURE_SIZE]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; SIGNATURE_SIZE] {
        &self.0
    }

    pub fn to_bytes(self) -> [u8; SIGNATURE_SIZE] {
        self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.as_bytes())
    }
}

impl fmt::Display for XSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

pub fn xeddsa_sign<RNG: RandProvider>(
    bytes: &[u8],
    secret_key: &SecretKey,
    rng: &RNG,
) -> Result<XSignature, XEdDSAError<RNG>> {
    let random_bytes: [u8; SIGNATURE_SIZE] =
        rng.random_array().map_err(|err| XEdDSAError::Rand(err))?;

    // calculate_key_pair
    let key_data = secret_key.as_bytes();
    let a = Scalar::from_bytes_mod_order(*key_data);
    let ed_public_key_point = &a * ED25519_BASEPOINT_TABLE;
    let ed_public_key = ed_public_key_point.compress();
    let sign_bit = ed_public_key.as_bytes()[31] & 0b1000_0000_u8;

    // r = hash1(a || M || Z) (mod q)
    let r = Scalar::from_bytes_mod_order_wide(&{
        // Explicitly pass a slice to avoid generating multiple versions of update().
        sha2_512(&[&HASH_1_PREFIX[..], &key_data[..], bytes, &random_bytes[..]])
    });

    // R = rB
    let cap_r = (&r * ED25519_BASEPOINT_TABLE).compress();

    // h = hash(R || A || M) (mod q)
    let h = Scalar::from_bytes_mod_order_wide(&{
        sha2_512(&[cap_r.as_bytes(), ed_public_key.as_bytes(), bytes])
    });

    // s = r + ha (mod q)
    let s = (h * a) + r;

    // return R || s
    let mut result = [0u8; SIGNATURE_SIZE];
    result[..32].copy_from_slice(cap_r.as_bytes());
    result[32..].copy_from_slice(s.as_bytes());
    result[SIGNATURE_SIZE - 1] &= 0b0111_1111_u8;
    result[SIGNATURE_SIZE - 1] |= sign_bit;
    Ok(XSignature::from_bytes(result))
}

pub fn xeddsa_verify<RNG: RandProvider>(
    bytes: &[u8],
    their_public_key: &PublicKey,
    signature: &XSignature,
) -> Result<(), XEdDSAError<RNG>> {
    let signature = signature.as_bytes();

    // if u >= p or R.y >= 2|p| or s >= 2|q|:
    //     return false
    // A = convert_mont(u)
    // if not on_curve(A):
    //     return false
    let mont_point = MontgomeryPoint(their_public_key.to_bytes());
    let ed_pub_key_point =
        match mont_point.to_edwards((signature[SIGNATURE_SIZE - 1] & 0b1000_0000_u8) >> 7) {
            Some(x) => x,
            None => return Err(XEdDSAError::InvalidArgument),
        };
    let cap_a = ed_pub_key_point.compress();
    let mut cap_r = [0u8; 32];
    cap_r.copy_from_slice(&signature[..32]);
    let mut s = [0u8; 32];
    s.copy_from_slice(&signature[32..]);
    s[31] &= 0b0111_1111_u8;
    if (s[31] & 0b1110_0000_u8) != 0 {
        return Err(XEdDSAError::InvalidArgument);
    }
    let minus_cap_a = -ed_pub_key_point;

    // h = hash(R || A || M) (mod q)
    let h = Scalar::from_bytes_mod_order_wide(&{
        // Explicitly pass a slice to avoid generating multiple versions of update().
        sha2_512(&[&cap_r[..], cap_a.as_bytes(), bytes])
    });

    // Rcheck = sB - hA
    // if bytes_equal(R, Rcheck):
    //     return true
    // return false
    let cap_r_check_point = EdwardsPoint::vartime_double_scalar_mul_basepoint(
        &h,
        &minus_cap_a,
        &Scalar::from_bytes_mod_order(s),
    );
    let cap_r_check = cap_r_check_point.compress();
    if bool::from(cap_r_check.as_bytes().ct_eq(&cap_r)) {
        Ok(())
    } else {
        Err(XEdDSAError::VerificationFailed)
    }
}

#[derive(Debug, Error)]
pub enum XEdDSAError<RNG: RandProvider> {
    #[error(transparent)]
    Rand(RNG::Error),

    #[error("invalid xeddsa public key or signature")]
    InvalidArgument,

    #[error("signature does not match public key and bytes")]
    VerificationFailed,
}

#[cfg(test)]
mod tests {
    use crate::crypto::{Crypto, SecretKey, XEdDSAError};
    use crate::traits::RandProvider;

    use super::{xeddsa_sign, xeddsa_verify};

    #[test]
    fn xeddsa_signatures() {
        let rng = Crypto::from_seed([1; 32]);

        let secret_key = SecretKey::from_bytes(rng.random_array().unwrap());
        let public_key = secret_key.public_key().unwrap();

        let signature = xeddsa_sign(b"Hello, Panda!", &secret_key, &rng).unwrap();
        assert!(xeddsa_verify::<Crypto>(b"Hello, Panda!", &public_key, &signature).is_ok());
    }

    #[test]
    fn failed_verify() {
        let rng = Crypto::from_seed([1; 32]);

        let secret_key = SecretKey::from_bytes(rng.random_array().unwrap());
        let public_key = secret_key.public_key().unwrap();
        let signature = xeddsa_sign(b"Hello, Panda!", &secret_key, &rng).unwrap();

        let invalid_secret_key = SecretKey::from_bytes(rng.random_array().unwrap());
        let invalid_public_key = invalid_secret_key.public_key().unwrap();
        let invalid_signature = xeddsa_sign(b"Hello, Panda!", &invalid_secret_key, &rng).unwrap();

        assert_ne!(public_key, invalid_public_key);
        assert_ne!(signature, invalid_signature);

        assert!(matches!(
            xeddsa_verify::<Crypto>(b"Invalid Data", &public_key, &signature),
            Err(XEdDSAError::VerificationFailed)
        ));
        assert!(matches!(
            xeddsa_verify::<Crypto>(b"Hello, Panda!", &invalid_public_key, &signature),
            Err(XEdDSAError::VerificationFailed)
        ));
        assert!(matches!(
            xeddsa_verify::<Crypto>(b"Hello, Panda!", &public_key, &invalid_signature),
            Err(XEdDSAError::VerificationFailed)
        ));
    }
}
