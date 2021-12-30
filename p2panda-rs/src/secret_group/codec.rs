// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;
use std::io::{Read, Write};

use openmls::framing::MlsCiphertext;
use tls_codec::{TlsDeserialize, TlsSerialize, TlsSize};

use crate::secret_group::lts::LongTermSecretCiphertext;
use crate::secret_group::SecretGroupMessage;

/// `SecretGroupMessageType` is an additional "helper" enum next to the `SecretGroupMessage` enum.
/// It is used as the first byte to distinct the type of the inner message data during TLS en- /
/// decoding.
#[derive(Debug, Clone, Copy, TlsSerialize, TlsDeserialize, TlsSize)]
#[repr(u8)]
enum SecretGroupMessageType {
    /// This message contains user data encrypted with a MLS sender ratchet secret and encoded in
    /// form of a MLS application message.
    SenderRatchetSecret = 1,

    /// This message contains user data encrypted with a long-term secret and encoded as a
    /// long-term secret ciphertext.
    LongTermSecret = 2,
}

impl TryFrom<u8> for SecretGroupMessageType {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(SecretGroupMessageType::SenderRatchetSecret),
            2 => Ok(SecretGroupMessageType::LongTermSecret),
            _ => Err("Unknown secret group message type."),
        }
    }
}

impl tls_codec::Size for SecretGroupMessage {
    #[inline]
    fn tls_serialized_len(&self) -> usize {
        SecretGroupMessageType::SenderRatchetSecret.tls_serialized_len()
            + match self {
                SecretGroupMessage::SenderRatchetSecret(message) => message.tls_serialized_len(),
                SecretGroupMessage::LongTermSecret(message) => message.tls_serialized_len(),
            }
    }
}

impl tls_codec::Serialize for SecretGroupMessage {
    fn tls_serialize<W: Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        match self {
            SecretGroupMessage::SenderRatchetSecret(message) => {
                // Write first byte indicating message type
                let written = SecretGroupMessageType::SenderRatchetSecret.tls_serialize(writer)?;

                // Write message data
                message.tls_serialize(writer).map(|l| l + written)
            }
            SecretGroupMessage::LongTermSecret(message) => {
                // Write first byte indicating message type
                let written = SecretGroupMessageType::LongTermSecret.tls_serialize(writer)?;

                // Write message data
                message.tls_serialize(writer).map(|l| l + written)
            }
        }
    }
}

impl tls_codec::Deserialize for SecretGroupMessage {
    fn tls_deserialize<R: Read>(bytes: &mut R) -> Result<Self, tls_codec::Error> {
        // Read the first byte to find out the message type
        let message_type = match SecretGroupMessageType::try_from(u8::tls_deserialize(bytes)?) {
            Ok(message_type) => message_type,
            Err(error) => {
                return Err(tls_codec::Error::DecodingError(format!(
                    "Deserialisation error {}",
                    error
                )))
            }
        };

        // Translate into enum and decode inner values
        match message_type {
            SecretGroupMessageType::SenderRatchetSecret => Ok(Self::SenderRatchetSecret(
                MlsCiphertext::tls_deserialize(bytes)?,
            )),
            SecretGroupMessageType::LongTermSecret => Ok(Self::LongTermSecret(
                LongTermSecretCiphertext::tls_deserialize(bytes)?,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use tls_codec::{Deserialize, Serialize};

    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::secret_group::lts::{
        LongTermSecret, LongTermSecretCiphersuite, LongTermSecretEpoch,
    };
    use crate::secret_group::MlsProvider;
    use crate::secret_group::{SecretGroup, SecretGroupCommit, SecretGroupMember};

    use super::SecretGroupMessage;

    #[test]
    fn secret_and_message() {
        let provider = MlsProvider::new();

        let random_key =
            hex::decode("fb5abbe6c223ab21fa92ba20aff944cd392af764b2df483d6d77cbdb719b76da")
                .unwrap();

        // Create long-term secret
        let secret = LongTermSecret::new(
            Hash::new_from_bytes(vec![1, 2, 3]).unwrap(),
            LongTermSecretCiphersuite::PANDA10_AES256GCM,
            LongTermSecretEpoch::default(),
            random_key.into(),
        );

        // Encrypt message with secret and wrap it inside `SecretGroupMessage`
        let nonce = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let ciphertext = secret
            .encrypt(&provider, &nonce, b"Secret message")
            .unwrap();
        let message = SecretGroupMessage::LongTermSecret(ciphertext);

        // Encode and decode secret
        let encoded = secret.tls_serialize_detached().unwrap();
        let decoded = LongTermSecret::tls_deserialize(&mut encoded.as_slice()).unwrap();
        assert_eq!(decoded, secret);

        // Encode and decode message
        let encoded = message.tls_serialize_detached().unwrap();
        let decoded = SecretGroupMessage::tls_deserialize(&mut encoded.as_slice()).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn epoch() {
        let epoch = LongTermSecretEpoch::default();

        // Encode and decode epoch
        let encoded = epoch.tls_serialize_detached().unwrap();
        let decoded = LongTermSecretEpoch::tls_deserialize(&mut encoded.as_slice()).unwrap();
        assert_eq!(decoded, epoch);
    }

    #[test]
    fn ciphersuite() {
        // Encode and decode ciphersuite
        for ciphersuite in LongTermSecretCiphersuite::supported_ciphersuites() {
            let encoded = ciphersuite.tls_serialize_detached().unwrap();
            let decoded =
                LongTermSecretCiphersuite::tls_deserialize(&mut encoded.as_slice()).unwrap();
            assert_eq!(decoded, ciphersuite);
        }

        // Throws error when ciphersuite is unknown
        assert!(LongTermSecretCiphersuite::tls_deserialize(&mut vec![21].as_slice()).is_err());
    }

    #[test]
    fn commits() {
        let provider = MlsProvider::new();

        // Create secret group and invite second member to create commit message
        let billie_key_pair = KeyPair::new();
        let billie_member = SecretGroupMember::new(&provider, &billie_key_pair).unwrap();

        let ada_key_pair = KeyPair::new();
        let ada_member = SecretGroupMember::new(&provider, &ada_key_pair).unwrap();
        let ada_key_package = ada_member.key_package(&provider).unwrap();

        let secret_group_id = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
        let mut group = SecretGroup::new(&provider, &secret_group_id, &billie_member).unwrap();
        let commit = group.add_members(&provider, &[ada_key_package]).unwrap();

        // Encode and decode commit
        let encoded = commit.tls_serialize_detached().unwrap();
        let decoded = SecretGroupCommit::tls_deserialize(&mut encoded.as_slice()).unwrap();
        assert_eq!(decoded.long_term_secrets(), commit.long_term_secrets());
        assert_eq!(decoded.welcome(), commit.welcome());

        // Apply decoded commit
        let ada_group = SecretGroup::new_from_welcome(&provider, &decoded).unwrap();
        assert!(ada_group.is_active());
    }
}
