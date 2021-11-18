// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use tls_codec::{TlsDeserialize, TlsSerialize, TlsSize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TlsDeserialize, TlsSerialize, TlsSize)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum LongTermSecretCiphersuite {
    PANDA_AES256GCMSIV = 0x01,
}
