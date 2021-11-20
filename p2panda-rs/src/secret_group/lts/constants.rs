// SPDX-License-Identifier: AGPL-3.0-or-later

/// Used label for exporting new secrets from the MLS group which are then used as Long Term Secret
/// keys.
pub const LTS_EXPORTER_LABEL: &str = "long_term_secret";

/// Length of the exported secret.
pub const LTS_EXPORTER_LENGTH: usize = 32; // AES256 key with 32 byte or 256bit
