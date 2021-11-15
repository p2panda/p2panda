// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::ciphersuite::Ciphersuite;
use openmls::prelude::{
    Credential, CredentialBundle, CredentialType, Extension, KeyPackage, KeyPackageBundle,
    LifetimeExtension,
};
use openmls_traits::key_store::OpenMlsKeyStore;
use openmls_traits::OpenMlsCryptoProvider;

use crate::encryption::mls::MlsProvider;
use crate::encryption::mls::{MLS_CIPHERSUITE_NAME, MLS_LIFETIME_EXTENSION};
use crate::identity::KeyPair;

#[derive(Debug)]
pub struct MlsMember {
    credential_bundle: CredentialBundle,
    provider: MlsProvider,
}

impl MlsMember {
    pub fn new(key_pair: KeyPair) -> Self {
        let ciphersuite = Ciphersuite::new(MLS_CIPHERSUITE_NAME).unwrap();

        // The identity of the Credential is the p2panda Author
        let public_key_bytes = key_pair.public_key().to_bytes();

        // Prepare crypto and storage backend for this member
        let provider = MlsProvider::new(key_pair);

        // Check if CredentialBundle already exists in store, otherwise generate it
        let credential_bundle = match provider.key_store().read(&public_key_bytes.to_vec()) {
            None => {
                // A CredentialBundle contains a Credential and the corresponding private key.
                // BasicCredential is a raw, unauthenticated assertion of an identity/key binding.
                let bundle = CredentialBundle::new(
                    public_key_bytes.to_vec(),
                    CredentialType::Basic,
                    ciphersuite.signature_scheme(),
                    &provider,
                )
                .unwrap();

                // Persist CredentialBundle in key store for the future
                provider
                    .key_store()
                    .store(bundle.credential().signature_key(), &bundle)
                    .unwrap();

                bundle
            }
            Some(bundle) => bundle,
        };

        Self {
            credential_bundle,
            provider,
        }
    }

    pub fn provider(&self) -> &impl OpenMlsCryptoProvider {
        &self.provider
    }

    /// Returns credentials of this group member which are used to identify it.
    pub fn credential(&self) -> &Credential {
        self.credential_bundle.credential()
    }

    /// Returns a KeyPackage of this group member.
    ///
    /// A KeyPackage object specifies a ciphersuite that the client supports, as well as
    /// providing a public key that others can use for key agreement.
    pub fn key_package(&self) -> KeyPackage {
        // The lifetime extension represents the times between which clients will consider a
        // KeyPackage valid. Its use is mandatory in the MLS specification
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

        // Retreive KeyPackage from bundle which is the public part of it
        let key_package = key_package_bundle.key_package().clone();
        let key_package_hash = key_package.hash(&self.provider);

        // Save generated bundle in key-store
        self.provider
            .key_store()
            .store(&key_package_hash, &key_package_bundle)
            .unwrap();

        // Finally return the public part
        key_package
    }
}

#[cfg(test)]
mod test {
    use crate::identity::KeyPair;

    use super::MlsMember;

    #[test]
    fn public_key_identity() {
        let key_pair = KeyPair::new();
        let public_key_bytes = key_pair.public_key().to_bytes();
        let member = MlsMember::new(key_pair);

        assert_eq!(public_key_bytes.to_vec(), member.credential().identity());
    }
}
