use openmls::group::GroupId;

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
    pub fn new(key_pair: KeyPair) -> Self {
        // Initialize MLS member instance with p2panda key pair
        let mls_member = MlsMember::new(key_pair);

        // Create an MLS group with random group id
        let group_id = GroupId::random(mls_member.provider());
        let mls_group = MlsGroup::new(group_id, &mls_member);

        Self {
            mls_member,
            mls_group,
        }
    }

    pub fn group_id(&self) -> Vec<u8> {
        self.mls_group.group_id().as_slice().to_vec()
    }
}

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum SymmetricalCiphersuite {
    PANDA_AES256GCMSIV = 0x0001,
}

#[derive(Debug)]
struct SymmetricalSecret {
    ciphersuite: SymmetricalCiphersuite,
    value: Vec<u8>,
    epoch: u64,
}

#[derive(Debug)]
pub struct SymmetricalMessage {
    group_id: Vec<u8>,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
    epoch: u64,
}

#[derive(Debug)]
pub struct SymmetricalEncryptionGroup {
    secrets: Vec<SymmetricalSecret>,
    encryption_group: EncryptionGroup,
}

impl SymmetricalEncryptionGroup {
    fn epoch_secret(&self, epoch: u64) -> &SymmetricalSecret {
        self.secrets
            .iter()
            .find(|secret| secret.epoch == epoch)
            .unwrap()
    }

    pub fn new(key_pair: KeyPair) -> Self {
        let mut encryption_group = EncryptionGroup::new(key_pair);

        let aes_secret = encryption_group.mls_group.export_secret(
            encryption_group.mls_member.provider(),
            AES_EXPORTER_LABEL,
            AES_EXPORTER_KEY_LENGTH,
        );

        let secret = SymmetricalSecret {
            ciphersuite: SymmetricalCiphersuite::PANDA_AES256GCMSIV,
            value: aes_secret,
            epoch: 0,
        };

        let mut secrets = Vec::new();
        secrets.push(secret);

        Self {
            secrets,
            encryption_group,
        }
    }

    pub fn encrypt_secret(&mut self) -> Vec<u8> {
        let secret = self.secrets.last().expect("Can not be empty!");

        let encrypted_aes_secret = self
            .encryption_group
            .mls_group
            .encrypt(self.encryption_group.mls_member.provider(), &secret.value);

        encrypted_aes_secret.ciphertext().to_vec()
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> SymmetricalMessage {
        let secret = self.secrets.last().expect("Can not be empty!");

        match secret.ciphersuite {
            SymmetricalCiphersuite::PANDA_AES256GCMSIV => {
                let (ciphertext, nonce) = aes::encrypt(&secret.value, plaintext).unwrap();
                let group_id = self.encryption_group.group_id();

                SymmetricalMessage {
                    group_id,
                    epoch: secret.epoch,
                    ciphertext,
                    nonce,
                }
            }
            _ => {
                panic!("Unknown ciphersuite");
            }
        }
    }

    pub fn decrypt(&self, message: &SymmetricalMessage) -> Vec<u8> {
        // @TODO: Throw error when epoch is missing
        let secret = self.epoch_secret(message.epoch);

        match secret.ciphersuite {
            SymmetricalCiphersuite::PANDA_AES256GCMSIV => {
                aes::decrypt(&secret.value, &message.nonce, &message.ciphertext).unwrap()
            }
            _ => {
                panic!("Unknown ciphersuite");
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::identity::KeyPair;

    use super::{EncryptionGroup, SymmetricalEncryptionGroup};

    #[test]
    fn test() {
        let key_pair = KeyPair::new();
        let group = SymmetricalEncryptionGroup::new(key_pair);
        let result = group.encrypt(b"This is a secret message");
        let plaintext = group.decrypt(&result);

        assert_eq!(result.epoch, 0);
        assert_eq!(b"This is a secret message".to_vec(), plaintext);
    }
}
