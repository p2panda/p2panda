// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::framing::MlsMessageOut;

use crate::secret_group::lts::LongTermSecretCiphertext;

/// Container around encrypted messages to distinct if they contain an MLS application message or
/// user data encrypted with a long-term secret.
#[derive(Debug, Clone, PartialEq)]
pub enum SecretGroupMessage {
    /// This message contains user data encrypted with a MLS sender ratchet secret and encoded in
    /// form of a MLS application message.
    SenderRatchetSecret(MlsMessageOut),

    /// This message contains user data encrypted with a long-term secret and encoded as a
    /// long-term secret ciphertext.
    LongTermSecret(LongTermSecretCiphertext),
}
