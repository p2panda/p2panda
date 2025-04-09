// SPDX-License-Identifier: MIT OR Apache-2.0

//! SHA2 hashing functions.
use sha2::{Digest, Sha512};

pub const SHA512_DIGEST_SIZE: usize = 64;

/// SHA2-512 hashing function.
pub fn sha2_512(messages: &[&[u8]]) -> [u8; SHA512_DIGEST_SIZE] {
    let mut hasher = Sha512::new();
    for message in messages {
        hasher.update(message);
    }
    let result = hasher.finalize();
    result[..].try_into().expect("sha512 digest size")
}
