// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use tls_codec::{TlsDeserialize, TlsSerialize, TlsSize};

#[derive(Debug, Serialize, Deserialize, TlsDeserialize, TlsSerialize, TlsSize)]
#[repr(u8)]
pub enum LongTermSecretCiphersuite {
    PANDA_AES256GCMSIV = 0x01,
}
