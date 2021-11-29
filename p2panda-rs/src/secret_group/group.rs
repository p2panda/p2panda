// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::framing::{MlsMessageIn, MlsMessageOut, VerifiableMlsPlaintext};
use openmls::group::GroupId;
use openmls::prelude::KeyPackage;
use openmls_traits::OpenMlsCryptoProvider;
use tls_codec::{Deserialize, Serialize, TlsVecU32};

use crate::hash::Hash;
use crate::secret_group::lts::{
    LongTermSecret, LongTermSecretCiphersuite, LongTermSecretEpoch, LongTermSecretNonce,
    LTS_DEFAULT_CIPHERSUITE, LTS_EXPORTER_LABEL, LTS_NONCE_EXPORTER_LABEL,
};
use crate::secret_group::mls::MlsGroup;
use crate::secret_group::{
    SecretGroupCommit, SecretGroupError, SecretGroupMember, SecretGroupMessage,
};

type LongTermSecretVec = TlsVecU32<LongTermSecret>;

/// Create or join secret groups, maintain their state and en- / decrypt user messages.
#[derive(Debug)]
pub struct SecretGroup {
    /// Used ciphersuite when generating new long term secrets
    long_term_ciphersuite: LongTermSecretCiphersuite,

    /// Internal counter for AEAD nonce.
    long_term_nonce: LongTermSecretNonce,

    /// Stored long term secrets (AEAD keys).
    long_term_secrets: LongTermSecretVec,

    /// Messaging Layer Security (MLS) group.
    mls_group: MlsGroup,

    /// Flag indicating if group was created by us.
    owned: bool,
}

impl SecretGroup {
    // Creation
    // ========

    /// Creates a new `SecretGroup` instance which can be used to encrypt data securely between
    /// members of the group.
    ///
    /// The first member of this group will automatically be the "owner" maintaining the group
    /// state by updating secrets, adding or removing members.
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
            // Hard code long term secret ciphersuite for now
            long_term_ciphersuite: LTS_DEFAULT_CIPHERSUITE,
            long_term_nonce: LongTermSecretNonce::default(),
            long_term_secrets: Vec::new().into(),
            mls_group,
            owned: true,
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
            long_term_ciphersuite: LTS_DEFAULT_CIPHERSUITE,
            long_term_nonce: LongTermSecretNonce::default(),
            long_term_secrets: Vec::new().into(),
            mls_group,
            owned: false,
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
    ///
    /// This method returns a `SecretGroupCommit` message which needs to be broadcasted in the
    /// network to then be downloaded and processed by all old and new group members to sync group
    /// state.
    pub fn add_members(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        key_packages: &[KeyPackage],
    ) -> Result<SecretGroupCommit, SecretGroupError> {
        if !self.owned {
            return Err(SecretGroupError::NotOwner);
        }

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
    ///
    /// This method returns a `SecretGroupCommit` message which needs to be broadcasted in the
    /// network to then be downloaded and processed by all other group members to sync group state.
    pub fn remove_members(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        // @TODO: Identify group members by p2panda public keys instead which we can internally
        // translate to key package hashes. Using key package hashes is part of the new MLS spec
        // and needs to be implemented in `openmls`.
        // See: https://github.com/openmls/openmls/issues/541
        member_leaf_indexes: &[usize],
    ) -> Result<SecretGroupCommit, SecretGroupError> {
        if !self.owned {
            return Err(SecretGroupError::NotOwner);
        }

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

    // Internal method for the group owner to process MLS Commit messages directly.
    //
    // According to the MLS specification commits would first be sent to a "Delivery Service" and
    // then processed after they got received again to assure correct ordering, but in the p2panda
    // case they need to be processed directly to be able to encrypt long term secrets based on the
    // new MLS group state. Also we don't have to worry about ordering here as commits are
    // organized by only one single append-only log (single-writer).
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

    /// Process an incoming `SecretGroupCommit` message to apply latest updates to the group.
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

    // Internal method to load long term secret from a certain epoch from the internal key store.
    fn long_term_secret(&self, epoch: LongTermSecretEpoch) -> Option<&LongTermSecret> {
        self.long_term_secrets
            .iter()
            .find(|secret| secret.long_term_epoch() == epoch)
    }

    // Reads an array of long term secrets and stores new ones when given. Ignores already existing
    // secrets.
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
    ///
    /// This new secret will initiate a new "epoch" and every message will be encrypted with this
    /// new secret from now on. Old long term secrets are kept and can  still be used to decrypt
    /// data from former epochs.
    ///
    /// Warning: Only group owners can rotate long term secrets.
    pub fn rotate_long_term_secret(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
    ) -> Result<(), SecretGroupError> {
        if !self.owned {
            return Err(SecretGroupError::NotOwner);
        }

        // Determine length of AEAD key.
        let key_length = self.long_term_ciphersuite.aead_key_length();

        // Generate secret key by using the MLS exporter method
        let value = self
            .mls_group
            .export_secret(provider, LTS_EXPORTER_LABEL, key_length)?;

        // Determine the epoch of the new secret
        let long_term_epoch = match self.long_term_epoch() {
            Some(mut epoch) => {
                epoch.increment();
                epoch
            }
            None => LongTermSecretEpoch::default(),
        };

        // Store secret in internal storage
        self.long_term_secrets.push(LongTermSecret::new(
            self.group_instance_id().clone(),
            self.long_term_ciphersuite,
            long_term_epoch,
            value.into(),
        ));

        Ok(())
    }

    // Encryption
    // ==========

    // Securely encodes and encrypts a list of long term secrets for the current MLS group. Members
    // of this MLS group epoch will be able to decrypt and use these secrets.
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

    // Generates unique nonce which can be used for AEAD.
    fn generate_nonce(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
    ) -> Result<Vec<u8>, SecretGroupError> {
        let public_key_str = hex::encode(self.mls_group.credential()?.identity());

        // Use constant value, public key and incrementing integer as exporter label
        let label = &format!(
            "{}{}{}",
            LTS_NONCE_EXPORTER_LABEL,
            &public_key_str,
            self.long_term_nonce.increment(),
        );

        // Determine length of AEAD nonce.
        let nonce_length = self.long_term_ciphersuite.aead_nonce_length();

        // Retreive nonce from MLS exporter
        let nonce = self
            .mls_group
            .export_secret(provider, label, nonce_length)?;

        Ok(nonce)
    }

    // Decrypts and decodes a list of long term secrets received via a commit message or when
    // joining an existing group.
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

    /// Encrypt user data using the a single-use "Sender Ratchet" secret generated by the current
    /// MLS group TreeKEM algorithm.
    ///
    /// This method gives forward-secrecy and post-compromise security. Use this encryption method
    /// for highly secure group settings and applications where decrypted data can safely be stored
    /// on the clients device since secrets are meant to be thrown away and not be reused, past
    /// data can not be recovered.
    pub fn encrypt(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        data: &[u8],
    ) -> Result<SecretGroupMessage, SecretGroupError> {
        let mls_ciphertext = self.mls_group.encrypt(provider, data)?;
        Ok(SecretGroupMessage::MlsApplicationMessage(mls_ciphertext))
    }

    /// Encrypt user data using the group's current symmetrical long term secret.
    ///
    /// This method gives only post-compromise security and has in general lower security
    /// guarantees but gives more flexibility. Use this encryption method if you want every old or
    /// new group member to decrypt past data even when they've joined the group later.
    pub fn encrypt_with_long_term_secret(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        data: &[u8],
    ) -> Result<SecretGroupMessage, SecretGroupError> {
        // Generate unique nonce for AES encryption
        let nonce = self.generate_nonce(provider)?;

        // Unwrap here since at this stage we already have at least one LTS epoch
        let epoch = self.long_term_epoch().unwrap();
        let secret = self.long_term_secret(epoch).unwrap();

        // Encrypt user data with last long term secret
        let ciphertext = secret.encrypt(provider, &nonce, data)?;

        Ok(SecretGroupMessage::LongTermSecretMessage(ciphertext))
    }

    /// Decrypts user data.
    ///
    /// This method automatically detects if the ciphertext was encrypted with a Sender Ratchet
    /// Secret or a Long Term Secret and returns an `SecretGroupError` if the required key material
    /// for decryption is missing.
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
                Ok(secret.decrypt(provider, ciphertext)?)
            }
        }
    }

    // Status
    // ======

    /// Returns true if this group is still active or false if the member got removed from the
    /// group.
    pub fn is_active(&self) -> bool {
        self.mls_group.is_active()
    }

    /// Returns true if this group is owned by us.
    pub fn is_owned(&self) -> bool {
        self.owned
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
    use crate::secret_group::{MlsProvider, SecretGroupMember, SecretGroupMessage};

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

    #[test]
    fn unique_exporter_nonce() {
        // Helper method to get nonce from SecretGroupMessage
        fn nonce(message: SecretGroupMessage) -> Vec<u8> {
            match message {
                SecretGroupMessage::LongTermSecretMessage(lts) => lts.nonce(),
                _ => panic!(),
            }
        }

        let group_instance_id = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
        let key_pair = KeyPair::new();
        let provider = MlsProvider::new();
        let member = SecretGroupMember::new(&provider, &key_pair).unwrap();
        let mut group = SecretGroup::new(&provider, &group_instance_id, &member).unwrap();

        let key_pair_2 = KeyPair::new();
        let member_2 = SecretGroupMember::new(&provider, &key_pair_2).unwrap();
        let key_package = member_2.key_package(&provider).unwrap();
        let commit = group.add_members(&provider, &[key_package]).unwrap();
        let mut group_2 = SecretGroup::new_from_welcome(&provider, &commit).unwrap();

        // Used nonces for LTS encryption should be unique for each message
        let ciphertext_1 = group
            .encrypt_with_long_term_secret(&provider, b"Secret")
            .unwrap();
        let ciphertext_2 = group
            .encrypt_with_long_term_secret(&provider, b"Secret")
            .unwrap();
        let ciphertext_3 = group_2
            .encrypt_with_long_term_secret(&provider, b"Secret")
            .unwrap();
        assert_ne!(nonce(ciphertext_1), nonce(ciphertext_2.clone()));
        assert_ne!(nonce(ciphertext_3), nonce(ciphertext_2));
    }

    #[test]
    fn group_ownership() {
        let group_instance_id = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
        let key_pair = KeyPair::new();
        let provider = MlsProvider::new();
        let owner = SecretGroupMember::new(&provider, &key_pair).unwrap();
        let mut group = SecretGroup::new(&provider, &group_instance_id, &owner).unwrap();
        assert!(group.is_owned());

        let key_pair_2 = KeyPair::new();
        let member = SecretGroupMember::new(&provider, &key_pair_2).unwrap();
        let key_package = member.key_package(&provider).unwrap();
        let commit = group.add_members(&provider, &[key_package]).unwrap();
        let mut group_2 = SecretGroup::new_from_welcome(&provider, &commit).unwrap();
        assert!(!group_2.is_owned());

        // Invited member does not have permission to change group.
        assert!(group_2.remove_members(&provider, &[1]).is_err());
        assert!(group_2.rotate_long_term_secret(&provider).is_err());
        assert!(group.rotate_long_term_secret(&provider).is_ok());
    }
}
