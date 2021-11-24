// SPDX-License-Identifier: AGPL-3.0-or-later

use tls_codec::{TlsDeserialize, TlsSerialize, TlsSize};

/// List of currently supported ciphersuites for Long Term Secret encryption.
#[derive(Debug, Clone, PartialEq, Copy, TlsDeserialize, TlsSerialize, TlsSize)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum LongTermSecretCiphersuite {
    PANDA_AES256GCMSIV = 0x01,
}
