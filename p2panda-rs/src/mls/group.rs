use openmls::ciphersuite::{Ciphersuite, CiphersuiteName};
use openmls::group::{GroupId, ManagedGroup, ManagedGroupConfig};
use openmls::prelude::{
    CredentialBundle, CredentialType, Extension, KeyPackageBundle, LifetimeExtension, WireFormat,
};
use openmls_traits::key_store::OpenMlsKeyStore;
use openmls_traits::OpenMlsCryptoProvider;

use crate::mls::MlsProvider;

pub const MLS_CIPHERSUITE_NAME: CiphersuiteName =
    CiphersuiteName::MLS10_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

pub const MLS_PADDING_SIZE: usize = 128;

pub const MLS_LIFETIME_EXTENSION: u64 = 60;

/// Wrapper around Managed MLS Group.
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

    pub fn new() -> Self {
        let provider = MlsProvider::new();

        let ciphersuite = Ciphersuite::new(MLS_CIPHERSUITE_NAME).unwrap();

        // A CredentialBundle contains a Credential and the corresponding private key.
        let credential_bundle = CredentialBundle::new(
            // The identity of this Credential is the p2panda Author
            vec![1, 2, 3],
            // A BasicCredential is a raw, unauthenticated assertion of an identity/key binding.
            CredentialType::Basic,
            ciphersuite.signature_scheme(),
            &provider,
        )
        .unwrap();

        let lifetime_extension =
            Extension::LifeTime(LifetimeExtension::new(MLS_LIFETIME_EXTENSION));

        let key_package_bundle = KeyPackageBundle::new(
            &[MLS_CIPHERSUITE_NAME],
            &credential_bundle,
            &provider,
            vec![lifetime_extension],
        )
        .unwrap();

        let key_package = key_package_bundle.key_package();
        let key_package_hash = key_package.hash(&provider);

        provider
            .key_store()
            .store(&key_package_hash, &key_package_bundle)
            .unwrap();

        let group = ManagedGroup::new(
            &provider,
            &Self::config(),
            GroupId::random(&provider),
            &key_package.hash(&provider),
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
    use super::MlsGroup;

    #[test]
    fn is_active() {
        let group = MlsGroup::new();
        assert_eq!(group.is_active(), true);
    }
}
