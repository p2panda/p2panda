// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::framing::MlsCiphertext;

use crate::secret_group::lts::LongTermSecretCiphertext;

pub enum SecretGroupMessage {
    MlsApplicationMessage(MlsCiphertext),
    LongTermSecretMessage(LongTermSecretCiphertext),
}
