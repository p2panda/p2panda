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

#[derive(Debug)]
pub struct SecretGroup {
    mls_group: MlsGroup,
    long_term_secrets: LongTermSecretVec,
}

impl SecretGroup {
    // Creation
    // ========

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

    fn long_term_secret(&self, epoch: LongTermSecretEpoch) -> Option<&LongTermSecret> {
        self.long_term_secrets
            .iter()
            .find(|secret| secret.long_term_epoch() == epoch)
    }

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

    pub fn rotate_long_term_secret(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
    ) -> Result<(), SecretGroupError> {
        let value =
            self.mls_group
                .export_secret(provider, LTS_EXPORTER_LABEL, LTS_EXPORTER_LENGTH)?;

        let long_term_epoch = match self.long_term_epoch() {
            Some(mut epoch) => {
                epoch.increment();
                epoch
            }
            None => LongTermSecretEpoch(0),
        };

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

    fn decrypt_long_term_secrets(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        encrypted_long_term_secrets: SecretGroupMessage,
    ) -> Result<LongTermSecretVec, SecretGroupError> {
        // Decrypt long term secrets with current group state
        let secrets_bytes = self.decrypt(provider, &encrypted_long_term_secrets)?;

        // Decode secrets
        let secrets = LongTermSecretVec::tls_deserialize(&mut secrets_bytes.as_slice())
            .map_err(|_| SecretGroupError::LTSEncodingError)?;

        Ok(secrets)
    }

    pub fn encrypt(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        data: &[u8],
    ) -> Result<SecretGroupMessage, SecretGroupError> {
        let mls_ciphertext = self.mls_group.encrypt(provider, data)?;
        Ok(SecretGroupMessage::MlsApplicationMessage(mls_ciphertext))
    }

    pub fn encrypt_with_long_term_secret(
        &self,
        data: &[u8],
    ) -> Result<SecretGroupMessage, SecretGroupError> {
        // Unwrap here since at this stage we already have at least one epoch
        let epoch = self.long_term_epoch().unwrap();
        let secret = self
            .long_term_secret(epoch)
            .ok_or_else(|| SecretGroupError::LTSSecretMissing)?;
        let ciphertext = secret.encrypt(data)?;
        Ok(SecretGroupMessage::LongTermSecretMessage(ciphertext))
    }

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
                    .long_term_secret(ciphertext.long_term_epoch)
                    .ok_or_else(|| SecretGroupError::LTSSecretMissing)?;
                Ok(secret.decrypt(ciphertext)?)
            }
        }
    }

    // Status
    // ======

    pub fn is_active(&self) -> bool {
        self.mls_group.is_active()
    }

    pub fn group_instance_id(&self) -> Hash {
        let group_id_bytes = self.mls_group.group_id().as_slice().to_vec();
        // Unwrap here since we already trusted the user input
        Hash::new_from_bytes(group_id_bytes).unwrap()
    }

    pub fn long_term_epoch(&self) -> Option<LongTermSecretEpoch> {
        self.long_term_secrets
            .iter()
            .map(|secret| secret.long_term_epoch())
            .max()
    }
}
