// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls_traits::types::Ciphersuite;

/// MLS ciphersuite used by p2panda.
///
/// A ciphersuite is a combination of a protocol version and the set of cryptographic algorithms
/// that should be used.
///
/// * MLS: The string "MLS"
/// * LVL: The security level
/// * KEM: The KEM algorithm used for HPKE in TreeKEM group operations
/// * AEAD: The AEAD algorithm used for HPKE and message protection
/// * HASH: The hash algorithm used for HPKE and the MLS transcript hash
/// * SIG: The Signature algorithm used for message authentication
pub const MLS_CIPHERSUITE_NAME: Ciphersuite =
    Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

/// The padding mechanism is used to improve protection against traffic analysis. The final size of
/// the ciphertext will be a multiple of the given padding.
pub const MLS_PADDING_SIZE: usize = 16;

/// The lifetime extension represents the times between which clients will consider a KeyPackage
/// valid.
pub const MLS_LIFETIME_EXTENSION_DAYS: u64 = 60; // 60 days
