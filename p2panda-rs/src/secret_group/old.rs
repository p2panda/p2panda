use std::io::{Read, Write};

use tls_codec::{Deserialize, Serialize, TlsByteVecU8, TlsDeserialize, TlsSerialize, TlsSize};

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

// ==========================================================
