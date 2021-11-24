// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::framing::{MlsMessageIn, MlsMessageOut, VerifiableMlsPlaintext};
use openmls::group::GroupId;
use openmls::prelude::KeyPackage;
use openmls_traits::OpenMlsCryptoProvider;
use tls_codec::{Deserialize, Serialize, TlsVecU32};

use crate::hash::Hash;
use crate::secret_group::lts::{
    LongTermSecret, LongTermSecretCiphersuite, LongTermSecretEpoch, LTS_EXPORTER_LABEL,
    LTS_EXPORTER_LENGTH,
};
use crate::secret_group::mls::MlsGroup;
use crate::secret_group::{
    SecretGroupCommit, SecretGroupError, SecretGroupMember, SecretGroupMessage,
};

type LongTermSecretVec = TlsVecU32<LongTermSecret>;

/// Main struct maintaining the MLS group- and LongTermSecret state, en- & decrypts user data and
/// processes SecretGroupCommit messages.
#[derive(Debug)]
pub struct SecretGroup {
    mls_group: MlsGroup,
    long_term_secrets: LongTermSecretVec,
}

impl SecretGroup {
    // Creation
    // ========

    /// Creates a new `SecretGroup` instance which can be used to encrypt data securely between
    /// members of the group.
    pub fn new(
        provider: &impl OpenMlsCryptoProvider,
        group_instance_id: &Hash,
        member: &SecretGroupMember,
    ) -> Result<Self, SecretGroupError> {
        // Generate new InitKeys and consume them directly when creating MLS group
        let init_key_package = member.key_package(provider)?;

        // Internally we use the MLS `GroupId` struct to represent groups since it already
        // implements the TLS encoding traits
        let group_id = GroupId::from_slice(&group_instance_id.to_bytes());

        // Create the MLS group with first member inside
        let mls_group = MlsGroup::new(provider, group_id, init_key_package)?;

        let mut group = Self {
            mls_group,
            long_term_secrets: Vec::new().into(),
        };

        // Generate first long term secret and store it in secret group
        group.rotate_long_term_secret(provider)?;

        Ok(group)
    }

    /// Creates a `SecretGroup` instance by joining an already existing group.
    pub fn new_from_welcome(
        provider: &impl OpenMlsCryptoProvider,
        commit: &SecretGroupCommit,
    ) -> Result<Self, SecretGroupError> {
        // Read MLS welcome from secret group commit and try to establish group state from it
        let mls_group = MlsGroup::new_from_welcome(
            provider,
            commit
                .welcome()
                .ok_or_else(|| SecretGroupError::WelcomeMissing)?,
        )?;

        let mut group = Self {
            mls_group,
            long_term_secrets: Vec::new().into(),
        };

        // Decode long term secrets with current group state
        let secrets = group.decrypt_long_term_secrets(provider, commit.long_term_secrets())?;

        // .. and finally add new secrets to group
        group.process_long_term_secrets(secrets)?;

        Ok(group)
    }

    // Membership
    // ==========

    /// Add new members to the group.
    pub fn add_members(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        key_packages: &[KeyPackage],
    ) -> Result<SecretGroupCommit, SecretGroupError> {
        // Add members
        let (mls_message_out, mls_welcome) = self.mls_group.add_members(provider, key_packages)?;

        // Process message directly to establish group state for encryption
        self.process_commit_directly(provider, &mls_message_out)?;

        // Re-Encrypt long term secrets for this group epoch
        let encrypt_long_term_secrets = self.encrypt_long_term_secrets(provider)?;

        Ok(SecretGroupCommit::new(
            mls_message_out,
            Some(mls_welcome),
            encrypt_long_term_secrets,
        )?)
    }

    /// Remove members from the group.
    pub fn remove_members(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        // @TODO: Identify group members by p2panda public keys instead which we can internally
        // translate to key package hashes. Using key package hashes is part of the new MLS spec
        // and needs to be implemented in `openmls`.
        // See: https://github.com/openmls/openmls/issues/541
        member_leaf_indexes: &[usize],
    ) -> Result<SecretGroupCommit, SecretGroupError> {
        // Remove members
        let mls_message_out = self
            .mls_group
            .remove_members(provider, member_leaf_indexes)?;

        // Process message directly to establish group state for encryption
        self.process_commit_directly(provider, &mls_message_out)?;

        // Re-Encrypt long term secrets for this group epoch
        let encrypt_long_term_secrets = self.encrypt_long_term_secrets(provider)?;

        Ok(SecretGroupCommit::new(
            mls_message_out,
            None,
            encrypt_long_term_secrets,
        )?)
    }

    // Commits
    // =======

    /// Internal method to process MLS Commit messages directly.
    ///
    /// Usually MLS Commits would first be sent to a "Delivery Service" and then processed after
    /// they got received again but in the p2panda case they need to be processed directly to be
    /// able to encrypt long term secrets based on the new MLS group state.
    fn process_commit_directly(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        mls_message_out: &MlsMessageOut,
    ) -> Result<(), SecretGroupError> {
        // Convert "out" to "in" message
        let mls_commit_message = match mls_message_out {
            MlsMessageOut::Plaintext(message) => Ok(MlsMessageIn::Plaintext(
                VerifiableMlsPlaintext::from_plaintext(message.clone(), None),
            )),
            _ => Err(SecretGroupError::NeedsToBeMlsPlaintext),
        }?;

        self.mls_group
            .process_commit(provider, mls_commit_message)?;

        Ok(())
    }

    /// Process an incoming `SecretGroupCommit` message.
    pub fn process_commit(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        commit: &SecretGroupCommit,
    ) -> Result<(), SecretGroupError> {
        // Apply commit message first
        self.mls_group.process_commit(provider, commit.commit())?;

        // Is this member still part of the group after the commit?
        if self.mls_group.is_active() {
            // Decrypt long term secrets with current group state
            let secrets = self.decrypt_long_term_secrets(provider, commit.long_term_secrets())?;

            // Add new secrets to group
            self.process_long_term_secrets(secrets)?;
        }

        Ok(())
    }

    // Long Term secrets
    // =================

    /// Loads a long term secret from a certain epoch from the internal key store.
    fn long_term_secret(&self, epoch: LongTermSecretEpoch) -> Option<&LongTermSecret> {
        self.long_term_secrets
            .iter()
            .find(|secret| secret.long_term_epoch() == epoch)
    }

    /// Reads an array of long term secrets and stores new ones when given. Ignores already
    /// existing secrets.
    fn process_long_term_secrets(
        &mut self,
        secrets: LongTermSecretVec,
    ) -> Result<(), SecretGroupError> {
        secrets.iter().try_for_each(|secret| {
            let group_instance_id = secret.group_instance_id()?;

            if self.group_instance_id() != group_instance_id {
                return Err(SecretGroupError::LTSInvalidGroupID);
            }

            if self.long_term_secret(secret.long_term_epoch()).is_none() {
                self.long_term_secrets.push(secret.clone());
            }

            Ok(())
        })?;

        Ok(())
    }

    /// Generates a new long term secret for this group.
    pub fn rotate_long_term_secret(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
    ) -> Result<(), SecretGroupError> {
        // Generate secret key by using the MLS exporter method
        let value =
            self.mls_group
                .export_secret(provider, LTS_EXPORTER_LABEL, LTS_EXPORTER_LENGTH)?;

        // Determine the epoch of the new secret
        let long_term_epoch = match self.long_term_epoch() {
            Some(mut epoch) => {
                epoch.increment();
                epoch
            }
            None => LongTermSecretEpoch(0),
        };

        // Store secret in internal storage
        self.long_term_secrets.push(LongTermSecret::new(
            self.group_instance_id().clone(),
            LongTermSecretCiphersuite::PANDA_AES256GCMSIV,
            long_term_epoch,
            value.into(),
        ));

        Ok(())
    }

    // Encryption
    // ==========

    /// Securely encodes and encrypts a list of long term secrets for the current MLS group.
    /// Members of this group will be able to decrypt and use these secrets.
    fn encrypt_long_term_secrets(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
    ) -> Result<SecretGroupMessage, SecretGroupError> {
        // Encode all long term secrets
        let encoded_secrets = self
            .long_term_secrets
            .tls_serialize_detached()
            .map_err(|_| SecretGroupError::LTSEncodingError)?;

        // Encrypt encoded secrets
        Ok(self.encrypt(provider, &encoded_secrets)?)
    }

    /// Decrypts and decodes a list of long term secrets.
    fn decrypt_long_term_secrets(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        encrypted_long_term_secrets: SecretGroupMessage,
    ) -> Result<LongTermSecretVec, SecretGroupError> {
        // Decrypt long term secrets with current group state
        let secrets_bytes = self.decrypt(provider, &encrypted_long_term_secrets)?;

        // Decode secrets
        let secrets = LongTermSecretVec::tls_deserialize(&mut secrets_bytes.as_slice())
            .map_err(|_| SecretGroupError::LTSDecodingError)?;

        Ok(secrets)
    }

    /// Encrypt user data asymmetrically using the current MLS group state.
    ///
    /// This method gives forward-secrecy and post-compromise security. Use this encryption method
    /// if decrypted data can be safely stored on the clients device since past data can not be
    /// recovered.
    pub fn encrypt(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        data: &[u8],
    ) -> Result<SecretGroupMessage, SecretGroupError> {
        let mls_ciphertext = self.mls_group.encrypt(provider, data)?;
        Ok(SecretGroupMessage::MlsApplicationMessage(mls_ciphertext))
    }

    /// Encrypt user data symmetrically using the current long term secret.
    ///
    /// This method gives only post-compromise security and has in general lower security
    /// guarantees but gives more flexibility. Use this encryption method if you want every old or
    /// new group member to decrypt past data even when they've joined the group later.
    pub fn encrypt_with_long_term_secret(
        &self,
        provider: &impl OpenMlsCryptoProvider,
        data: &[u8],
    ) -> Result<SecretGroupMessage, SecretGroupError> {
        // Unwrap here since at this stage we already have at least one LTS epoch
        let epoch = self.long_term_epoch().unwrap();
        let secret = self.long_term_secret(epoch).unwrap();

        // Encrypt user data with last long term secret
        let ciphertext = secret.encrypt(provider, data)?;

        Ok(SecretGroupMessage::LongTermSecretMessage(ciphertext))
    }

    /// Decrypt user data.
    ///
    /// This method automatically detects if the ciphertext was encrypted with MLS or a long term
    /// secret.
    pub fn decrypt(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        message: &SecretGroupMessage,
    ) -> Result<Vec<u8>, SecretGroupError> {
        match message {
            SecretGroupMessage::MlsApplicationMessage(ciphertext) => {
                Ok(self.mls_group.decrypt(provider, ciphertext.clone())?)
            }
            SecretGroupMessage::LongTermSecretMessage(ciphertext) => {
                let secret = self
                    .long_term_secret(ciphertext.long_term_epoch())
                    .ok_or_else(|| SecretGroupError::LTSSecretMissing)?;
                Ok(secret.decrypt(ciphertext)?)
            }
        }
    }

    // Status
    // ======

    /// Returns true if this group is still active, or false if the member got removed from the
    /// group.
    pub fn is_active(&self) -> bool {
        self.mls_group.is_active()
    }

    /// Returns the hash of this `SecretGroup` instance.
    pub fn group_instance_id(&self) -> Hash {
        let group_id_bytes = self.mls_group.group_id().as_slice().to_vec();
        // Unwrap here since we already trusted the user input
        Hash::new_from_bytes(group_id_bytes).unwrap()
    }

    /// Returns the current epoch of the long term secret.
    pub fn long_term_epoch(&self) -> Option<LongTermSecretEpoch> {
        self.long_term_secrets
            .iter()
            .map(|secret| secret.long_term_epoch())
            .max()
    }
}

#[cfg(test)]
mod tests {
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::secret_group::lts::LongTermSecretEpoch;
    use crate::secret_group::{MlsProvider, SecretGroupMember};

    use super::SecretGroup;

    #[test]
    fn group_lts_epochs() {
        let group_instance_id = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
        let key_pair = KeyPair::new();
        let provider = MlsProvider::new();
        let member = SecretGroupMember::new(&provider, &key_pair).unwrap();
        let mut group = SecretGroup::new(&provider, &group_instance_id, &member).unwrap();

        // Epochs increment with every newly generated Long Term Secret
        assert_eq!(group.long_term_epoch(), Some(LongTermSecretEpoch(0)));
        group.rotate_long_term_secret(&provider).unwrap();
        assert_eq!(group.long_term_epoch(), Some(LongTermSecretEpoch(1)));
        group.rotate_long_term_secret(&provider).unwrap();
        assert_eq!(group.long_term_epoch(), Some(LongTermSecretEpoch(2)));
    }
}
