// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::framing::{MlsCiphertext, MlsMessageIn, MlsMessageOut};
use openmls::group::{GroupEvent, GroupId, ManagedGroup, ManagedGroupConfig};
use openmls::prelude::{Credential, KeyPackage, Welcome, WireFormat};
use openmls_traits::OpenMlsCryptoProvider;

use crate::secret_group::mls::{MlsError, MLS_PADDING_SIZE};

/// Wrapper around the Managed MLS Group of `openmls`.
#[derive(Debug)]
pub struct MlsGroup(ManagedGroup);

impl MlsGroup {
    /// Returns a p2panda specific configuration for MLS Groups
    fn config() -> ManagedGroupConfig {
        ManagedGroupConfig::builder()
            // Handshake messages should not be encrypted
            .wire_format(WireFormat::MlsPlaintext)
            // Size of padding in bytes
            .padding_size(MLS_PADDING_SIZE)
            // Flag to indicate the Ratchet Tree Extension should be used, otherwise we would need
            // to tell clients via an external solution about the current Rachet Tree. Read more in
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
        // Retreive hash from key package
        let key_package_hash = key_package.hash(provider);

        // Create MLS group with one member inside
        let group = ManagedGroup::new(provider, &Self::config(), group_id, &key_package_hash)?;

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
        let group = ManagedGroup::new_from_welcome(provider, &Self::config(), welcome, None)?;
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
        Ok(self.0.add_members(provider, members)?)
    }

    /// Removes members from a MLS group which results in a Commit message.
    ///
    /// Please note that this current implementation requires the member leaf indexes to identify
    /// the to-be-removed members which will be changed in the future.
    ///
    /// See: https://github.com/openmls/openmls/issues/541
    pub fn remove_members(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        member_leaf_indexes: &[usize],
    ) -> Result<MlsMessageOut, MlsError> {
        let results = self.0.remove_members(provider, member_leaf_indexes)?;

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
    ) -> Result<Vec<GroupEvent>, MlsError> {
        // @TODO: This API will change soon in `openmls`.
        // See: https://github.com/openmls/openmls/pull/576
        Ok(self.0.process_message(message, provider)?)
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
    ) -> Result<MlsCiphertext, MlsError> {
        let message = self.0.create_message(provider, data)?;

        // @TODO: This should be handled internally by `openmls` instead:
        // https://github.com/openmls/openmls/issues/584
        match message {
            MlsMessageOut::Ciphertext(ciphertext) => Ok(ciphertext),
            _ => panic!("Expected MLS ciphertext"),
        }
    }

    /// Decrypts data with the current known MLS group secrets.
    ///
    /// In this implementation the data has to be an Application message as Handshake messages are
    /// not encrypted in p2panda.
    pub fn decrypt(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        ciphertext: MlsCiphertext,
    ) -> Result<Vec<u8>, MlsError> {
        // @TODO: This API will change soon in `openmls`.
        // See: https://github.com/openmls/openmls/pull/576
        let group_events = self
            .0
            .process_message(MlsMessageIn::Ciphertext(ciphertext), provider)?;

        match group_events.last() {
            Some(GroupEvent::ApplicationMessage(application_message_event)) => {
                Ok(application_message_event.message().to_owned())
            }
            _ => panic!("Expected an ApplicationMessage event"),
        }
    }

    // Status
    // ======

    /// Returns the group id.
    pub fn group_id(&self) -> &GroupId {
        self.0.group_id()
    }

    /// Returns the own `Credential` in this group or an Error when member was already removed.
    pub fn credential(&self) -> Result<Credential, MlsError> {
        Ok(self.0.credential()?)
    }

    /// Returns true if the group is still active for this member (maybe it has been removed or
    /// left the group).
    pub fn is_active(&self) -> bool {
        self.0.is_active()
    }

    /// Return members of MLS group.
    pub fn members(&self) -> Result<Vec<Credential>, MlsError> {
        Ok(self.0.members())
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

        let member = MlsMember::new(&provider, &key_pair).unwrap();
        let group_id = GroupId::random(&provider);
        let key_package = member.key_package(&provider).unwrap();
        let mut group = MlsGroup::new(&provider, group_id, key_package).unwrap();
        assert_eq!(group.is_active(), true);

        let message = "This is a very secret message";
        let ciphertext = group.encrypt(&provider, message.as_bytes()).unwrap();
        let plaintext = group.decrypt(&provider, ciphertext).unwrap();
        assert_eq!(&plaintext, message.as_bytes());
    }
}
