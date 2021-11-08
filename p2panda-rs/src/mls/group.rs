use openmls::ciphersuite::{Ciphersuite, CiphersuiteName};
use openmls::group::{GroupId, ManagedGroup, ManagedGroupConfig};
use openmls::prelude::{
    CredentialBundle, CredentialType, Extension, KeyPackageBundle, LifetimeExtension, WireFormat,
};
use openmls_rust_crypto::OpenMlsRustCrypto;

pub const MLS_CIPHERSUITE_NAME: CiphersuiteName =
    CiphersuiteName::MLS10_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

pub const MLS_PADDING_SIZE: usize = 128;

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
        let crypto = OpenMlsRustCrypto::default();

        let ciphersuite = Ciphersuite::new(MLS_CIPHERSUITE_NAME).unwrap();

        let credential_bundle = CredentialBundle::new(
            vec![1, 2, 3],
            CredentialType::Basic,
            ciphersuite.signature_scheme(),
            &crypto,
        )
        .unwrap();

        let lifetime_extension = Extension::LifeTime(LifetimeExtension::new(60));

        let key_package_bundle = KeyPackageBundle::new(
            &[MLS_CIPHERSUITE_NAME],
            &credential_bundle,
            &crypto,
            vec![lifetime_extension],
        )
        .unwrap();

        let key_package = key_package_bundle.key_package();

        let group = ManagedGroup::new(
            &crypto,
            &Self::config(),
            GroupId::random(&crypto),
            &key_package.hash(&crypto),
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
    fn test() {
        let group = MlsGroup::new();
        assert_eq!(group.is_active(), true);
    }
}
