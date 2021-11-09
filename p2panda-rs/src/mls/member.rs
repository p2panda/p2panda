use openmls::ciphersuite::{Ciphersuite, CiphersuiteName};
use openmls::prelude::{
    Credential, CredentialBundle, CredentialType, Extension, KeyPackage, KeyPackageBundle,
    LifetimeExtension, WireFormat,
};
use openmls_traits::key_store::OpenMlsKeyStore;
use openmls_traits::OpenMlsCryptoProvider;

use crate::identity::KeyPair;
use crate::mls::MlsProvider;
use crate::mls::{MLS_CIPHERSUITE_NAME, MLS_LIFETIME_EXTENSION};

pub struct MlsMember {
    credential_bundle: CredentialBundle,
    provider: MlsProvider,
}

impl MlsMember {
    pub fn new(key_pair: KeyPair) -> Self {
        // The identity of the Credential is the p2panda Author
        let public_key_bytes = key_pair.public_key().to_bytes();

        let ciphersuite = Ciphersuite::new(MLS_CIPHERSUITE_NAME).unwrap();
        let provider = MlsProvider::new(key_pair);

        // A CredentialBundle contains a Credential and the corresponding private key.
        // BasicCredential is a raw, unauthenticated assertion of an identity/key binding.
        let credential_bundle = CredentialBundle::new(
            public_key_bytes.to_vec(),
            CredentialType::Basic,
            ciphersuite.signature_scheme(),
            &provider,
        )
        .unwrap();

        Self {
            credential_bundle,
            provider,
        }
    }

    pub fn provider(&self) -> &impl OpenMlsCryptoProvider {
        &self.provider
    }

    pub fn credential(&self) -> &Credential {
        self.credential_bundle.credential()
    }

    pub fn key_package(&self) -> KeyPackage {
        let lifetime_extension =
            Extension::LifeTime(LifetimeExtension::new(MLS_LIFETIME_EXTENSION));

        // KeyPackageBundles contain KeyPackage with the corresponding private HPKE (Hybrid Public
        // Key Encryption) key.
        let key_package_bundle = KeyPackageBundle::new(
            &[MLS_CIPHERSUITE_NAME],
            &self.credential_bundle,
            &self.provider,
            vec![lifetime_extension],
        )
        .unwrap();

        // A KeyPackage object specifies a ciphersuite that the client supports, as well as
        // providing a public key that others can use for key agreement. When used as InitKeys,
        // KeyPackages are intended to be used only once and SHOULD NOT be reused except in case of
        // last resort. This is why this key package here is generated and directly used without
        // publication.
        let key_package = key_package_bundle.key_package().clone();
        let key_package_hash = key_package.hash(&self.provider);
        self.provider
            .key_store()
            .store(&key_package_hash, &key_package_bundle)
            .unwrap();

        key_package
    }
}

#[cfg(test)]
mod test {
    use crate::identity::KeyPair;

    use super::MlsMember;

    #[test]
    fn is_active() {
        let key_pair = KeyPair::new();
        let public_key_bytes = key_pair.public_key().to_bytes();
        let member = MlsMember::new(key_pair);
        assert_eq!(
            public_key_bytes.to_vec(),
            member.credential().identity()
        );
    }
}
