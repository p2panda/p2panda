// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::ciphersuite::hash_ref::KeyPackageRef;
use openmls::credentials::Credential;
use openmls::framing::{MlsMessageIn, MlsMessageOut, ProcessedMessage};
use openmls::group::{GroupId, MlsGroup as Group, MlsGroupConfig};
use openmls::key_packages::KeyPackage;
use openmls::messages::Welcome;
use openmls::prelude::SenderRatchetConfiguration;
use openmls_traits::OpenMlsCryptoProvider;

use crate::secret_group::mls::{
    MlsError, MLS_MAX_FORWARD_DISTANCE, MLS_MAX_PAST_EPOCHS, MLS_OUT_OF_ORDER_TOLERANCE,
    MLS_PADDING_SIZE, MLS_WIRE_FORMAT_POLICY,
};

/// Wrapper around the Managed MLS Group of `openmls`.
#[derive(Debug)]
pub struct MlsGroup(Group);

impl MlsGroup {
    /// Returns a p2panda specific configuration for MLS Groups.
    fn config() -> MlsGroupConfig {
        let sender_ratchet_configuration =
            SenderRatchetConfiguration::new(MLS_OUT_OF_ORDER_TOLERANCE, MLS_MAX_FORWARD_DISTANCE);

        MlsGroupConfig::builder()
            // This allows application messages from previous epochs to be decrypted
            .max_past_epochs(MLS_MAX_PAST_EPOCHS)
            // Stores the configuration parameters for decryption ratchets
            .sender_ratchet_configuration(sender_ratchet_configuration)
            // Handshake messages should not be encrypted
            .wire_format_policy(MLS_WIRE_FORMAT_POLICY)
            // Size of padding in bytes
            .padding_size(MLS_PADDING_SIZE)
            // Flag to indicate the Ratchet Tree Extension should be used, otherwise we would need
            // to tell clients via an external solution about the current Ratchet Tree. Read more in
            // MLS specification Section 11.3.
            .use_ratchet_tree_extension(true)
            .build()
    }

    // Creation
    // ========

    /// Creates a new MLS group. A group is always created with a single member, the "creator".
    ///
    /// The given [KeyPackage] ("InitKeys") will directly be consumed during group creation and not
    /// further propagated.
    pub fn new(
        provider: &impl OpenMlsCryptoProvider,
        group_id: GroupId,
        key_package: KeyPackage,
    ) -> Result<Self, MlsError> {
        // Retrieve hash from key package
        let key_package_hash = key_package.hash_ref(provider.crypto())?;

        // Create MLS group with one member inside
        let group = Group::new(
            provider,
            &Self::config(),
            group_id,
            key_package_hash.as_slice(),
        )?;

        Ok(Self(group))
    }

    /// Joins an already existing MLS group through a Welcome message.
    ///
    /// New members can use the information inside the Welcome message to set up their own group
    /// state and derive a shared secret.
    pub fn new_from_welcome(
        provider: &impl OpenMlsCryptoProvider,
        welcome: Welcome,
    ) -> Result<Self, MlsError> {
        let group = Group::new_from_welcome(provider, &Self::config(), welcome, None)?;
        Ok(Self(group))
    }

    // Membership
    // ==========

    /// Adds new members to an existing MLS group which results in a Commit and Welcome message.
    ///
    /// The sender of a Commit message is responsible for sending a Welcome message to any new
    /// members. The Welcome message provides the new members with the current state of the group,
    /// after the application of the Commit message.
    pub fn add_members(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        members: &[KeyPackage],
    ) -> Result<(MlsMessageOut, Welcome), MlsError> {
        // Create staged commit with proposals to add members
        let message = self.0.add_members(provider, members)?;

        // Merge and process the staged commit directly, this advances the group epoch
        self.0.merge_pending_commit()?;

        Ok(message)
    }

    /// Removes members from a MLS group which results in a Commit message.
    pub fn remove_members(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        members: &[KeyPackageRef],
    ) -> Result<MlsMessageOut, MlsError> {
        // Create staged commit with proposals to remove members
        let results = self.0.remove_members(provider, members)?;

        // Merge and process the staged commit directly, this advances the group epoch
        self.0.merge_pending_commit()?;

        // MLS returns an `MlsMessageOut` and optional `Welcome` message when removing a member. We
        // can be sure there will be no Welcome message in the p2panda case, so we only take the
        // "out" message.
        Ok(results.0)
    }

    // Commits
    // =======

    /// Processes a Commit message.
    ///
    /// A Commit message initiates a new epoch for the group, based on a collection of Proposals.
    /// It instructs group members to update their representation of the state of the group by
    /// applying the proposals and advancing the key schedule.
    pub fn process_commit(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        message: MlsMessageIn,
    ) -> Result<(), MlsError> {
        // Check for syntactic errors
        let unverified_message = self.0.parse_message(message, provider)?;

        // Check for semantic errors
        let processed_message =
            self.0
                .process_unverified_message(unverified_message, None, provider)?;

        // Process the message finally and advance the group key schedule
        if let ProcessedMessage::StagedCommitMessage(staged_commit) = processed_message {
            self.0.merge_staged_commit(*staged_commit)?;
        } else {
            return Err(MlsError::UnexpectedMessage);
        }

        Ok(())
    }

    // Encryption
    // ==========

    /// Exports secrets based on the current MLS group epoch.
    ///
    /// The main MLS key schedule provides an exporter which can be used by an application as the
    /// basis to derive new secrets outside the MLS layer.
    pub fn export_secret(
        &self,
        provider: &impl OpenMlsCryptoProvider,
        label: &str,
        key_length: usize,
    ) -> Result<Vec<u8>, MlsError> {
        Ok(self.0.export_secret(provider, label, &[], key_length)?)
    }

    /// Encrypts data for each member of the group.
    pub fn encrypt(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        data: &[u8],
    ) -> Result<MlsMessageOut, MlsError> {
        Ok(self.0.create_message(provider, data)?)
    }

    /// Decrypts data with the current known MLS group secrets.
    ///
    /// In this implementation the data has to be an Application message as Handshake messages are
    /// not encrypted in p2panda.
    pub fn decrypt(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        message: MlsMessageIn,
    ) -> Result<Vec<u8>, MlsError> {
        // Check for syntactic errors and decrypt messsage
        let unverified_message = self.0.parse_message(message, provider)?;

        // Check for semantic errors
        let processed_message =
            self.0
                .process_unverified_message(unverified_message, None, provider)?;

        if let ProcessedMessage::ApplicationMessage(application_message) = processed_message {
            Ok(application_message.into_bytes())
        } else {
            Err(MlsError::UnexpectedMessage)
        }
    }

    // Status
    // ======

    /// Returns the group id.
    pub fn group_id(&self) -> &GroupId {
        self.0.group_id()
    }

    /// Returns the own `Credential` in this group or an Error when member was already removed.
    pub fn credential(&self) -> Result<&Credential, MlsError> {
        Ok(self.0.credential()?)
    }

    /// Returns true if the group is still active for this member (maybe it has been removed or
    /// left the group).
    pub fn is_active(&self) -> bool {
        self.0.is_active()
    }

    /// Return members of MLS group.
    pub fn members(&self) -> Vec<&KeyPackage> {
        self.0.members()
    }
}

#[cfg(test)]
mod tests {
    use openmls::group::GroupId;

    use crate::identity::KeyPair;
    use crate::secret_group::mls::{MlsMember, MlsProvider};

    use super::MlsGroup;

    #[test]
    fn group_encryption() {
        let key_pair = KeyPair::new();
        let provider = MlsProvider::new();

        // Create MLS group with one member
        let member = MlsMember::new(&provider, &key_pair).unwrap();
        let group_id = GroupId::random(&provider);
        let key_package = member.key_package(&provider).unwrap();
        let mut group = MlsGroup::new(&provider, group_id, key_package.clone()).unwrap();

        // Group is active and contains the owner of the group as the only member
        assert!(group.is_active());
        assert_eq!(group.members(), vec![&key_package]);

        // Owner can not decrypt its own messages (for forward secrecy)
        let message = "This is a very secret message";
        let ciphertext = group.encrypt(&provider, message.as_bytes()).unwrap();
        assert!(group.decrypt(&provider, ciphertext.into()).is_err());
    }
}
