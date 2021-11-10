use openmls::ciphersuite::CiphersuiteName;

/// A ciphersuite is a combination of a protocol version and the set of cryptographic algorithms
/// that should be used.
///
/// * MLS: The string "MLS" followed by the major and minor version, e.g. "MLS10"
/// * LVL: The security level
/// * KEM: The KEM algorithm used for HPKE in TreeKEM group operations
/// * AEAD: The AEAD algorithm used for HPKE and message protection
/// * HASH: The hash algorithm used for HPKE and the MLS transcript hash
/// * SIG: The Signature algorithm used for message authentication
pub const MLS_CIPHERSUITE_NAME: CiphersuiteName =
    CiphersuiteName::MLS10_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

/// The padding mechanism is used to improve protection against traffic analysis.
pub const MLS_PADDING_SIZE: usize = 128;

/// The lifetime extension represents the times between which clients will consider a KeyPackage
/// valid.
pub const MLS_LIFETIME_EXTENSION: u64 = 60;
