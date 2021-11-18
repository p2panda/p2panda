// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::group::GroupId;
use serde::{Deserialize, Serialize};
use tls_codec::{Size, TlsByteVecU8, TlsDeserialize, TlsSerialize, TlsSize};

use crate::secret_group::lts::LongTermSecretEpoch;

#[derive(Debug, Clone, Serialize, Deserialize, TlsDeserialize, TlsSerialize, TlsSize)]
pub struct LongTermSecretCiphertext {
    group_id: GroupId,
    long_term_epoch: LongTermSecretEpoch,
    ciphertext: TlsByteVecU8,
}
