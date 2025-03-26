// SPDX-License-Identifier: MIT OR Apache-2.0

//! SHA2 hashing functions.
use libcrux_traits::Digest;

pub const DIGEST_SIZE: usize = libcrux_sha2::SHA512_LENGTH;

/// SHA2-512 hashing function.
pub fn sha2_512(messages: &[&[u8]]) -> [u8; DIGEST_SIZE] {
    let mut hasher = libcrux_sha2::Sha512::new();
    for message in messages {
        hasher.update(message);
    }
    let mut output = [0u8; DIGEST_SIZE];
    hasher.finish(&mut output);
    output
}
