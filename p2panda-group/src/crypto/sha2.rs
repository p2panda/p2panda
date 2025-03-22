// SPDX-License-Identifier: MIT OR Apache-2.0

//! SHA2-512 hashing function.
use libcrux::digest::{Algorithm, Sha2_512, digest_size};
use libcrux_traits::Digest;

const ALGORITHM: Algorithm = Algorithm::Sha512;

pub const DIGEST_SIZE: usize = digest_size(ALGORITHM);

pub fn sha2_512(messages: &[&[u8]]) -> [u8; DIGEST_SIZE] {
    let mut hasher = Sha2_512::new();

    for message in messages {
        hasher.update(message);
    }

    let mut output = [0u8; DIGEST_SIZE];
    hasher.finish(&mut output);

    output
}
