// SPDX-License-Identifier: MIT OR Apache-2.0

//! SHA2 hashing functions.
use sha2::{Digest, Sha256, Sha512};

pub const SHA512_DIGEST_SIZE: usize = 64;

pub const SHA256_DIGEST_SIZE: usize = 32;

/// SHA2-512 hashing function.
pub fn sha2_512(messages: &[&[u8]]) -> [u8; SHA512_DIGEST_SIZE] {
    let mut hasher = Sha512::new();
    for message in messages {
        hasher.update(message);
    }
    let result = hasher.finalize();
    result[..].try_into().expect("sha512 digest size")
}

/// SHA2-256 hashing function.
pub fn sha2_256(messages: &[&[u8]]) -> [u8; SHA256_DIGEST_SIZE] {
    let mut hasher = Sha256::new();
    for message in messages {
        hasher.update(message);
    }
    let result = hasher.finalize();
    result[..].try_into().expect("sha256 digest size")
}
