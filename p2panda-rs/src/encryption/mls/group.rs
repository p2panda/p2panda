// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::framing::MlsCiphertext;
use openmls::group::{
    GroupEvent, GroupId, ManagedGroup, ManagedGroupConfig, MlsMessageIn, MlsMessageOut,
};
use openmls::prelude::WireFormat;
use openmls_traits::OpenMlsCryptoProvider;

use crate::encryption::mls::{MlsMember, MLS_PADDING_SIZE};

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

    /// Creates a new MLS group. A group is always created with a single member, the "creator".
    pub fn new(group_id: GroupId, member: &MlsMember) -> Self {
        // Generate a new KeyPackage which can be used to create the group (aka InitKeys). These
        // keys will directly be consumed during group creation and not further propagated.
        let key_package_hash = member.key_package().hash(member.provider());

        // Create MLS group with one member inside
        let group = ManagedGroup::new(
            member.provider(),
            &Self::config(),
            group_id,
            &key_package_hash,
        )
        .unwrap();

        Self(group)
    }

    /// Returns true if the group is still active for this member (maybe it has been removed or
    /// left the group).
    pub fn is_active(&self) -> bool {
        self.0.is_active()
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
}

#[cfg(test)]
mod test {
    use openmls::group::GroupId;

    use crate::identity::KeyPair;

    use super::{MlsGroup, MlsMember};

    #[test]
    fn is_active() {
        let key_pair = KeyPair::new();

        let member = MlsMember::new(key_pair);
        let group_id = GroupId::random(member.provider());
        let mut group = MlsGroup::new(group_id, &member);
        assert_eq!(group.is_active(), true);

        let message = "This is a very secret message";
        let ciphertext = group.encrypt(member.provider(), message.as_bytes());
        let plaintext = group.decrypt(member.provider(), ciphertext);
        assert_eq!(&plaintext, message.as_bytes());
    }
}
