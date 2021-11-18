// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::framing::{MlsCiphertext, MlsMessageIn, MlsMessageOut};
use openmls::group::{GroupEvent, GroupId, ManagedGroup, ManagedGroupConfig};
use openmls::prelude::{KeyPackage, Welcome, WireFormat};
use openmls_traits::OpenMlsCryptoProvider;

use crate::secret_group::mls::MLS_PADDING_SIZE;

/// Wrapper around the Managed MLS Group.
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
    /// The given KeyPackage ("InitKeys") will directly be consumed during group creation and not
    /// further propagated.
    pub fn new(
        provider: &impl OpenMlsCryptoProvider,
        group_id: GroupId,
        key_package: KeyPackage,
    ) -> Self {
        let key_package_hash = key_package.hash(provider);

        // Create MLS group with one member inside
        let group =
            ManagedGroup::new(provider, &Self::config(), group_id, &key_package_hash).unwrap();

        Self(group)
    }

    pub fn new_from_welcome(provider: &impl OpenMlsCryptoProvider, welcome: Welcome) -> Self {
        let group =
            ManagedGroup::new_from_welcome(provider, &Self::config(), welcome, None).unwrap();

        Self(group)
    }

    // Membership
    // ==========

    pub fn add_members(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        members: &[KeyPackage],
    ) -> (MlsMessageOut, Welcome) {
        self.0.add_members(provider, members).unwrap()
    }

    pub fn remove_members(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        member_leaf_indexes: &[usize],
    ) -> MlsMessageOut {
        self.0
            .remove_members(provider, member_leaf_indexes)
            .unwrap()
            .0
    }

    // Commits
    // =======

    pub fn process_commit(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        message: MlsMessageIn,
    ) -> Vec<GroupEvent> {
        self.0.process_message(message, provider).unwrap()
    }

    // Encryption
    // ==========

    pub fn export_secret(
        &self,
        provider: &impl OpenMlsCryptoProvider,
        label: &str,
        key_length: usize,
    ) -> Vec<u8> {
        self.0
            .export_secret(provider, label, &[], key_length)
            .unwrap()
    }

    pub fn encrypt(&mut self, provider: &impl OpenMlsCryptoProvider, data: &[u8]) -> MlsCiphertext {
        let message = self.0.create_message(provider, data).unwrap();

        match message {
            MlsMessageOut::Ciphertext(ciphertext) => ciphertext,
            _ => panic!("This will never happen"),
        }
    }

    pub fn decrypt(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        ciphertext: MlsCiphertext,
    ) -> Vec<u8> {
        let group_events = self
            .0
            .process_message(MlsMessageIn::Ciphertext(ciphertext), provider)
            .unwrap();

        match group_events.last() {
            Some(GroupEvent::ApplicationMessage(application_message_event)) => {
                application_message_event.message().to_owned()
            }
            _ => panic!("Expected an ApplicationMessage event"),
        }
    }

    // Status
    // ======

    pub fn group_id(&self) -> &GroupId {
        self.0.group_id()
    }

    /// Returns true if the group is still active for this member (maybe it has been removed or
    /// left the group).
    pub fn is_active(&self) -> bool {
        self.0.is_active()
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
        let provider = MlsProvider::new(key_pair);

        let member = MlsMember::new(&provider, b"test");
        let group_id = GroupId::random(&provider);
        let key_package = member.key_package(&provider);
        let mut group = MlsGroup::new(&provider, group_id, key_package);
        assert_eq!(group.is_active(), true);

        let message = "This is a very secret message";
        let ciphertext = group.encrypt(&provider, message.as_bytes());
        let plaintext = group.decrypt(&provider, ciphertext);
        assert_eq!(&plaintext, message.as_bytes());
    }
}
