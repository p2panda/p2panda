// SPDX-License-Identifier: AGPL-3.0-or-later

/// Used label for exporting new secrets from the MLS group which are then used as Long Term Secret
/// keys.
pub const LTS_EXPORTER_LABEL: &str = "long_term_secret";

/// Used label for exporting new AES GCM nonce.
pub const LTS_NONCE_EXPORTER_LABEL: &str = "long_term_nonce";

/// Length of the exported secret.
pub const LTS_EXPORTER_LENGTH: usize = 32; // AES256 key with 32 byte or 256bit

/// Length of the exported nonce.
pub const LTS_NONCE_EXPORTER_LENGTH: usize = 12; // AES256 nonce with 12 byte or 96bit
