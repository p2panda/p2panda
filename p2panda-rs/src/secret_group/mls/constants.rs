// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::group::{WireFormatPolicy, PURE_PLAINTEXT_WIRE_FORMAT_POLICY};
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

/// Defines the wire format policy for outgoing and incoming handshake messages. Application are
/// always encrypted regardless.
pub const MLS_WIRE_FORMAT_POLICY: WireFormatPolicy = PURE_PLAINTEXT_WIRE_FORMAT_POLICY;

/// This number sets the storage size of message secrets from past epochs.
///
/// It is a trade-off between functionality and forward secrecy and is used if we can not guarantee
/// that application messages will be sent in the same epoch in which they were generated.
pub const MLS_MAX_PAST_EPOCHS: usize = 8;

/// This parameter defines how many incoming messages can be skipped in case they have been
/// dropped, deleted or are missing.
pub const MLS_MAX_FORWARD_DISTANCE: u32 = 1024;

/// This parameter defines a window for which decryption secrets are kept.
///
/// This is useful in case we cannot guarantee that all application messages have total order
/// within an epoch. Use this carefully, since keeping decryption secrets affects forward secrecy
/// within an epoch.
pub const MLS_OUT_OF_ORDER_TOLERANCE: u32 = 16;
