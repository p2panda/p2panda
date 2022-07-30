// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::next::secret_group::lts::LongTermSecretCiphersuite;

/// Default long-term secret ciphersuite used by p2panda.
///
/// A ciphersuite is a combination of a protocol version and the set of cryptographic algorithms
/// that should be used.
///
/// * PANDA: The string "PANDA" followed by the major and minor version, e.g. "PANDA10"
/// * AEAD: The AEAD algorithm used for long-term secret encryption
pub const LTS_DEFAULT_CIPHERSUITE: LongTermSecretCiphersuite =
    LongTermSecretCiphersuite::PANDA10_AES256GCM;

/// Used label for exporting new secrets from the MLS group which are then used as Long Term Secret
/// keys.
pub const LTS_EXPORTER_LABEL: &str = "long_term_secret";

/// Used label for exporting new AEAD nonce.
pub const LTS_NONCE_EXPORTER_LABEL: &str = "long_term_nonce";
