use openmls::group::{GroupId, ManagedGroup, ManagedGroupConfig};
use openmls::prelude::{
    CredentialBundle, CredentialType, Extension, KeyPackageBundle, LifetimeExtension, WireFormat,
};
use openmls_traits::key_store::OpenMlsKeyStore;
use openmls_traits::OpenMlsCryptoProvider;

use crate::mls::{MlsMember, MlsProvider, MLS_PADDING_SIZE};

/// Wrapper around the Managed MLS Group.
#[derive(Debug)]
pub struct MlsGroup(ManagedGroup);

impl MlsGroup {
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
        let key_package_hash = member.key_package().hash(member.provider());

        let group = ManagedGroup::new(
            member.provider(),
            &Self::config(),
            group_id,
            &key_package_hash,
        )
        .unwrap();

        Self(group)
    }

    pub fn is_active(&self) -> bool {
        self.0.is_active()
    }
}

#[cfg(test)]
mod test {
    use openmls::group::GroupId;

    use super::{MlsGroup, MlsMember};

    #[test]
    fn is_active() {
        let member = MlsMember::new();
        let group_id = GroupId::random(member.provider());
        let group = MlsGroup::new(group_id, &member);
        assert_eq!(group.is_active(), true);
    }
}
