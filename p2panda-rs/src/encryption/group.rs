use std::io::{Read, Write};

use openmls::group::{GroupEpoch, GroupId};
use openmls::prelude::KeyPackage;
use tls_codec::{Deserialize, Serialize, TlsByteVecU8, TlsDeserialize, TlsSerialize, TlsSize};

use crate::encryption::aes;
use crate::encryption::mls::{MlsGroup, MlsMember};
use crate::identity::KeyPair;

const AES_EXPORTER_LABEL: &str = "aes_secret";
const AES_EXPORTER_KEY_LENGTH: usize = 32;

#[derive(Debug)]
pub struct EncryptionGroup {
    pub(self) mls_group: MlsGroup,
    pub(self) mls_member: MlsMember,
}

impl EncryptionGroup {
    pub fn new(mls_member: MlsMember) -> Self {
        // Create an MLS group with random group id
        let group_id = GroupId::random(mls_member.provider());
        let mls_group = MlsGroup::new(group_id, &mls_member);

        Self {
            mls_member,
            mls_group,
        }
    }

    pub fn group_id(&self) -> &GroupId {
        self.mls_group.group_id()
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    TlsDeserialize,
    TlsSerialize,
    TlsSize,
    serde::Deserialize,
    serde::Serialize,
)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum SymmetricalCiphersuite {
    PANDA_AES256GCMSIV = 0x01,
}

#[derive(Debug, PartialEq)]
pub struct SymmetricalSecret {
    ciphersuite: SymmetricalCiphersuite,
    epoch: GroupEpoch,
    value: TlsByteVecU8,
}

impl SymmetricalSecret {
    pub fn value(&self) -> Vec<u8> {
        self.value.as_slice().to_vec()
    }
}

impl tls_codec::Deserialize for SymmetricalSecret {
    fn tls_deserialize<R: Read>(bytes: &mut R) -> Result<Self, tls_codec::Error> {
        let ciphersuite = SymmetricalCiphersuite::tls_deserialize(bytes)?;
        let epoch = GroupEpoch::tls_deserialize(bytes)?;
        let value = TlsByteVecU8::tls_deserialize(bytes)?;

        Ok(SymmetricalSecret {
            ciphersuite,
            epoch,
            value,
        })
    }
}

impl tls_codec::Size for SymmetricalSecret {
    #[inline]
    fn tls_serialized_len(&self) -> usize {
        self.ciphersuite.tls_serialized_len()
            + self.epoch.tls_serialized_len()
            + self.value.tls_serialized_len()
    }
}

impl tls_codec::Serialize for SymmetricalSecret {
    #[inline]
    fn tls_serialize<W: Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        let mut written = self.ciphersuite.tls_serialize(writer)?;
        written += self.epoch.tls_serialize(writer)?;
        written += self.value.tls_serialize(writer)?;
        Ok(written)
    }
}

#[derive(Debug, PartialEq)]
pub struct SymmetricalMessage {
    group_id: GroupId,
    epoch: GroupEpoch,
    nonce: TlsByteVecU8,
    ciphertext: TlsByteVecU8,
}

impl SymmetricalMessage {
    pub fn ciphertext(&self) -> Vec<u8> {
        self.ciphertext.as_slice().to_vec()
    }

    pub fn nonce(&self) -> Vec<u8> {
        self.nonce.as_slice().to_vec()
    }
}

impl tls_codec::Deserialize for SymmetricalMessage {
    fn tls_deserialize<R: Read>(bytes: &mut R) -> Result<Self, tls_codec::Error> {
        let group_id = GroupId::tls_deserialize(bytes)?;
        let epoch = GroupEpoch::tls_deserialize(bytes)?;
        let nonce = TlsByteVecU8::tls_deserialize(bytes)?;
        let ciphertext = TlsByteVecU8::tls_deserialize(bytes)?;

        Ok(SymmetricalMessage {
            group_id,
            epoch,
            nonce,
            ciphertext,
        })
    }
}

impl tls_codec::Size for SymmetricalMessage {
    #[inline]
    fn tls_serialized_len(&self) -> usize {
        self.group_id.tls_serialized_len()
            + self.epoch.tls_serialized_len()
            + self.nonce.tls_serialized_len()
            + self.ciphertext.tls_serialized_len()
    }
}

impl tls_codec::Serialize for SymmetricalMessage {
    #[inline]
    fn tls_serialize<W: Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        let mut written = self.group_id.tls_serialize(writer)?;
        written += self.epoch.tls_serialize(writer)?;
        written += self.nonce.tls_serialize(writer)?;
        written += self.ciphertext.tls_serialize(writer)?;
        Ok(written)
    }
}

#[derive(Debug)]
pub struct SymmetricalEncryptionGroup {
    secrets: Vec<SymmetricalSecret>,
    encryption_group: EncryptionGroup,
}

impl SymmetricalEncryptionGroup {
    fn epoch_secret(&self, epoch: GroupEpoch) -> Option<&SymmetricalSecret> {
        self.secrets.iter().find(|secret| secret.epoch == epoch)
    }

    pub fn new(member: MlsMember) -> Self {
        let encryption_group = EncryptionGroup::new(member);

        let aes_secret = encryption_group.mls_group.export_secret(
            encryption_group.mls_member.provider(),
            AES_EXPORTER_LABEL,
            AES_EXPORTER_KEY_LENGTH,
        );

        let secret = SymmetricalSecret {
            ciphersuite: SymmetricalCiphersuite::PANDA_AES256GCMSIV,
            value: aes_secret.into(),
            epoch: GroupEpoch(0),
        };

        let mut secrets = Vec::new();
        secrets.push(secret);

        Self {
            secrets,
            encryption_group,
        }
    }

    pub fn encrypt_secret(&mut self) -> Vec<u8> {
        // Load latest secret
        let secret = self.secrets.last().expect("Can not be empty!");
        let encoded = secret.tls_serialize_detached().unwrap();

        // Encrypt AEAD secret with MLS
        let message = self
            .encryption_group
            .mls_group
            .encrypt(self.encryption_group.mls_member.provider(), &encoded);

        message
    }

    pub fn decrypt_secret(&mut self, ciphertext: Vec<u8>) -> SymmetricalSecret {
        let decoded = self
            .encryption_group
            .mls_group
            .decrypt(self.encryption_group.mls_member.provider(), ciphertext);

        SymmetricalSecret::tls_deserialize(&mut decoded.as_slice()).unwrap()
    }

    pub fn process_secret(&mut self, secret: SymmetricalSecret) {
        if self.epoch_secret(secret.epoch).is_none() {
            self.secrets.push(secret);
        }
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Vec<u8> {
        let secret = self.secrets.last().expect("Can not be empty!");

        match secret.ciphersuite {
            SymmetricalCiphersuite::PANDA_AES256GCMSIV => {
                let (ciphertext, nonce) = aes::encrypt(&secret.value(), plaintext).unwrap();
                let group_id = self.encryption_group.group_id().clone();

                let message = SymmetricalMessage {
                    group_id,
                    epoch: secret.epoch,
                    ciphertext: ciphertext.into(),
                    nonce: nonce.into(),
                };

                message.tls_serialize_detached().unwrap()
            }
        }
    }

    pub fn decrypt(&self, encoded_message: Vec<u8>) -> Vec<u8> {
        let message = SymmetricalMessage::tls_deserialize(&mut encoded_message.as_slice()).unwrap();

        // @TODO: Throw error when epoch is missing
        let secret = self.epoch_secret(message.epoch).unwrap();

        match secret.ciphersuite {
            SymmetricalCiphersuite::PANDA_AES256GCMSIV => {
                aes::decrypt(&secret.value(), &message.nonce(), &message.ciphertext()).unwrap()
            }
        }
    }
}

#[cfg(test)]
mod test {
    use openmls::group::{GroupEpoch, GroupId};
    use tls_codec::{Deserialize, Serialize};

    use crate::encryption::mls::MlsMember;
    use crate::identity::KeyPair;

    use super::{
        SymmetricalCiphersuite, SymmetricalEncryptionGroup, SymmetricalMessage, SymmetricalSecret,
    };

    #[test]
    fn aead_encryption() {
        let key_pair = KeyPair::new();
        let member = MlsMember::new(key_pair);
        let group = SymmetricalEncryptionGroup::new(member);
        let ciphertext = group.encrypt(b"This is a secret message");
        let plaintext = group.decrypt(ciphertext);
        assert_eq!(b"This is a secret message".to_vec(), plaintext);
    }

    #[test]
    fn aead_secret_encryption() {
        // Billie creates a group
        let key_pair = KeyPair::new();
        let member = MlsMember::new(key_pair);
        let mut group = SymmetricalEncryptionGroup::new(member);

        // Ada publishes their KeyPackage material
        let key_pair_2 = KeyPair::new();
        let member_2 = MlsMember::new(key_pair_2);
        let key_package = member_2.key_package();

        // Billie generates a new AEAD secret and publishes it
        // @TODO
        let secret_ciphertext = group.encrypt_secret();

        // Billie invites Ada into their group
        // @TODO

        // Ada joins the group
        // @TODO
        let mut group_2 = SymmetricalEncryptionGroup::new(member_2);

        // Ada reads and decrypts the published secret of Billie
        // @TODO
        let secret = group_2.decrypt_secret(secret_ciphertext);
        group_2.process_secret(secret);

        // Billie sends an symmetrically encrypted message to Ada
        let message_ciphertext = group.encrypt(b"This is a secret message");

        // Ada decrypts the message with the secret
        // @TODO
        let message = group_2.decrypt(message_ciphertext);

        assert_eq!(b"This is a secret message".to_vec(), message);
    }

    #[test]
    fn encoding() {
        // SymmetricalMessage
        let message = SymmetricalMessage {
            group_id: GroupId::from_slice(b"test"),
            epoch: GroupEpoch(12),
            nonce: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12].into(),
            ciphertext: vec![4, 5, 6].into(),
        };

        let encoded = message.tls_serialize_detached().unwrap();
        let decoded = SymmetricalMessage::tls_deserialize(&mut encoded.as_slice()).unwrap();
        assert_eq!(message, decoded);

        // SymmetricalSecret
        let secret = SymmetricalSecret {
            ciphersuite: SymmetricalCiphersuite::PANDA_AES256GCMSIV,
            epoch: GroupEpoch(12),
            value: vec![4, 12, 3, 6].into(),
        };

        let encoded = secret.tls_serialize_detached().unwrap();
        let decoded = SymmetricalSecret::tls_deserialize(&mut encoded.as_slice()).unwrap();
        assert_eq!(secret, decoded);
    }
}
