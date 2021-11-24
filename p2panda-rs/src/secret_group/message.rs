// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::framing::MlsCiphertext;

use crate::secret_group::lts::LongTermSecretCiphertext;

/// Container around encrypted messages to distinct if they contain a MLS application message or
/// user data encrypted with a long term secret.
#[derive(Debug, Clone, PartialEq)]
pub enum SecretGroupMessage {
    /// This message contains user data encrypted and encoded in form of a MLS application message.
    MlsApplicationMessage(MlsCiphertext),

    /// This message contains user data encrypted and encoded as a long term secret ciphertext.
    LongTermSecretMessage(LongTermSecretCiphertext),
}
