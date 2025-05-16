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

use crate::crypto::sha2::sha2_512;
use crate::crypto::x25519::{PublicKey, SecretKey};
use crate::crypto::{Rng, RngError};

/// 512-bit signature.
pub const SIGNATURE_SIZE: usize = 64;

/// Hash1 changes the first byte to 0xFE.
const HASH_1_PREFIX: [u8; 32] = [
    0xFEu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8,
    0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8,
    0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8, 0xFFu8,
];

/// XEdDSA signature.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

    pub fn to_hex(self) -> String {
        hex::encode(self.as_bytes())
    }
}

impl fmt::Display for XSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Calculates an XEdDSA signature using the X25519 secret key directly.
pub fn xeddsa_sign(
    bytes: &[u8],
    secret_key: &SecretKey,
    rng: &Rng,
) -> Result<XSignature, XEdDSAError> {
    // M = Message to sign (byte sequence)
    let cap_m = bytes;

    // Z = 64 bytes secure random data (byte sequence)
    let cap_z: [u8; SIGNATURE_SIZE] = rng.random_array()?;

    // A, a = calculate_key_pair(k)
    let (cap_a, a) = {
        // k = Montgomery private key (integer mod q)
        let k_bytes = secret_key.as_bytes();
        let k = Scalar::from_bytes_mod_order(*k_bytes);

        // calculate_key_pair(k)
        let cap_e = &k * ED25519_BASEPOINT_TABLE; // E = kB
        let mut cap_a = cap_e.compress(); // A.y = E.y
        let sign_bit = cap_a.0[31] >> 7; // sign_bit = E.s
        cap_a.0[31] &= 0b0111_1111_u8; // A.s = 0

        // if E.s == 1:
        //   a = -k (mod q)
        // else:
        //   a = k (mod q)
        let a = if sign_bit == 1 { -k } else { k };

        (cap_a, a)
    };

    // r = hash1(a || M || Z) (mod q)
    let r = Scalar::from_bytes_mod_order_wide(&{
        sha2_512(&[&HASH_1_PREFIX, a.as_bytes(), cap_m, &cap_z])
    });

    // R = rB
    let cap_r = (&r * ED25519_BASEPOINT_TABLE).compress();

    // h = hash(R || A || M) (mod q)
    let h = Scalar::from_bytes_mod_order_wide(&{
        sha2_512(&[cap_r.as_bytes(), cap_a.as_bytes(), cap_m])
    });

    // s = r + ha (mod q)
    let s = r + (h * a);

    // return R || s
    let mut result = [0u8; SIGNATURE_SIZE];
    result[..32].copy_from_slice(cap_r.as_bytes());
    result[32..].copy_from_slice(s.as_bytes());
    Ok(XSignature::from_bytes(result))
}

/// Verifies a XEdDSA signature on provided data using the X25519 public counter-part.
pub fn xeddsa_verify(
    bytes: &[u8],
    their_public_key: &PublicKey,
    signature: &XSignature,
) -> Result<(), XEdDSAError> {
    // M = Message to sign (byte sequence)
    let cap_m = bytes;

    // u = Montgomery public key (byte sequence of b bits).
    let u = their_public_key;

    // R || s = Signature to verify (byte sequence of 2b bits)
    let mut cap_r = [0u8; 32];
    cap_r.copy_from_slice(&signature.as_bytes()[..32]);
    let mut s = [0u8; 32];
    s.copy_from_slice(&signature.as_bytes()[32..]);
    s[31] &= 0b0111_1111_u8;

    // Reject s if it has excess bits.
    if (s[31] & 0b1110_0000_u8) != 0 {
        return Err(XEdDSAError::InvalidArgument);
    }

    // convert_mont(u):
    //   umasked = u (mod 2|p|)
    //   P.y = u_to_y(umasked)
    //   P.s = 0
    //   return P
    let a = {
        let mont_point = MontgomeryPoint(u.to_bytes());
        match mont_point.to_edwards(0) {
            Some(x) => x,
            // if not on_curve(A):
            //   return false
            None => return Err(XEdDSAError::InvalidArgument),
        }
    };
    let cap_a = a.compress();

    // h = hash(R || A || M) (mod q)
    let h = Scalar::from_bytes_mod_order_wide(&{ sha2_512(&[&cap_r, cap_a.as_bytes(), cap_m]) });

    // Rcheck = sB - hA
    let cap_r_check = {
        let minus_cap_a = -a;
        let cap_r_check_point = EdwardsPoint::vartime_double_scalar_mul_basepoint(
            &h,
            &minus_cap_a,
            &Scalar::from_bytes_mod_order(s),
        );
        cap_r_check_point.compress()
    };

    // if bytes_equal(R, Rcheck):
    //   return true
    if bool::from(cap_r_check.as_bytes().ct_eq(&cap_r)) {
        Ok(())
    } else {
        Err(XEdDSAError::VerificationFailed)
    }
}

#[derive(Debug, Error)]
pub enum XEdDSAError {
    #[error(transparent)]
    Rng(#[from] RngError),

    #[error("invalid xeddsa public key or signature")]
    InvalidArgument,

    #[error("signature does not match public key and bytes")]
    VerificationFailed,
}

#[cfg(test)]
mod tests {
    use crate::crypto::Rng;
    use crate::crypto::x25519::SecretKey;

    use super::{XEdDSAError, xeddsa_sign, xeddsa_verify};

    #[test]
    fn xeddsa_signatures() {
        let rng = Rng::from_seed([1; 32]);

        let secret_key = SecretKey::from_bytes(rng.random_array().unwrap());
        let public_key = secret_key.public_key().unwrap();

        let signature = xeddsa_sign(b"Hello, Panda!", &secret_key, &rng).unwrap();
        assert!(xeddsa_verify(b"Hello, Panda!", &public_key, &signature).is_ok());
    }

    #[test]
    fn failed_verify() {
        let rng = Rng::from_seed([1; 32]);

        let secret_key = SecretKey::from_bytes(rng.random_array().unwrap());
        let public_key = secret_key.public_key().unwrap();
        let signature = xeddsa_sign(b"Hello, Panda!", &secret_key, &rng).unwrap();

        let invalid_secret_key = SecretKey::from_bytes(rng.random_array().unwrap());
        let invalid_public_key = invalid_secret_key.public_key().unwrap();
        let invalid_signature = xeddsa_sign(b"Hello, Panda!", &invalid_secret_key, &rng).unwrap();

        assert_ne!(public_key, invalid_public_key);
        assert_ne!(signature, invalid_signature);

        assert!(matches!(
            xeddsa_verify(b"Invalid Data", &public_key, &signature),
            Err(XEdDSAError::VerificationFailed)
        ));
        assert!(matches!(
            xeddsa_verify(b"Hello, Panda!", &invalid_public_key, &signature),
            Err(XEdDSAError::VerificationFailed)
        ));
        assert!(matches!(
            xeddsa_verify(b"Hello, Panda!", &public_key, &invalid_signature),
            Err(XEdDSAError::VerificationFailed)
        ));
    }
}
