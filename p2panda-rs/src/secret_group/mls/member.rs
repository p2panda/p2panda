// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::ciphersuite::Ciphersuite;
use openmls::prelude::{
    Credential, CredentialBundle, Extension, KeyPackage, KeyPackageBundle, LifetimeExtension,
    SignatureKeypair,
};
use openmls_traits::key_store::OpenMlsKeyStore;
use openmls_traits::OpenMlsCryptoProvider;

use crate::identity::KeyPair;
use crate::secret_group::mls::{MLS_CIPHERSUITE_NAME, MLS_LIFETIME_EXTENSION};

#[derive(Debug, Clone)]
pub struct MlsMember {
    credential_bundle: CredentialBundle,
}

impl MlsMember {
    pub fn new(provider: &impl OpenMlsCryptoProvider, key_pair: &KeyPair) -> Self {
        let ciphersuite = Ciphersuite::new(MLS_CIPHERSUITE_NAME).unwrap();

        // Credential identities are p2panda public keys!
        let public_key = key_pair.public_key().to_bytes();

        // Check if CredentialBundle already exists in store, otherwise generate it
        let credential_bundle = match provider.key_store().read(&public_key) {
            None => {
                // Full key here because we need it to sign
                let private_key = key_pair.private_key().to_bytes();
                let full_key = [private_key, public_key].concat();

                let signature_key_pair = SignatureKeypair::from_bytes(
                    ciphersuite.signature_scheme(),
                    full_key.to_vec(),
                    public_key.to_vec(),
                );

                // A CredentialBundle contains a Credential and the corresponding private key.
                // BasicCredential is a raw, unauthenticated assertion of an identity/key binding.
                let bundle = CredentialBundle::from_parts(
                    public_key.to_vec(),
                    signature_key_pair,
                );

                // Persist CredentialBundle in key store for the future
                provider
                    .key_store()
                    .store(bundle.credential().signature_key(), &bundle)
                    .unwrap();

                bundle
            }
            Some(bundle) => bundle,
        };

        Self { credential_bundle }
    }

    /// Returns credentials of this group member which are used to identify it.
    pub fn credential(&self) -> &Credential {
        self.credential_bundle.credential()
    }

    /// Returns a KeyPackage of this group member.
    ///
    /// A KeyPackage object specifies a ciphersuite that the client supports, as well as
    /// providing a public key that others can use for key agreement.
    pub fn key_package(&self, provider: &impl OpenMlsCryptoProvider) -> KeyPackage {
        // The lifetime extension represents the times between which clients will consider a
        // KeyPackage valid. Its use is mandatory in the MLS specification
        let lifetime_extension =
            Extension::LifeTime(LifetimeExtension::new(MLS_LIFETIME_EXTENSION));

        // KeyPackageBundles contain KeyPackage with the corresponding private HPKE (Hybrid Public
        // Key Encryption) key.
        let key_package_bundle = KeyPackageBundle::new(
            &[MLS_CIPHERSUITE_NAME],
            &self.credential_bundle,
            provider,
            vec![lifetime_extension],
        )
        .unwrap();

        // Retreive KeyPackage from bundle which is the public part of it
        let key_package = key_package_bundle.key_package().clone();
        let key_package_hash = key_package.hash(provider);

        // Save generated bundle in key-store
        provider
            .key_store()
            .store(&key_package_hash, &key_package_bundle)
            .unwrap();

        // Finally return the public part
        key_package
    }
}

#[cfg(test)]
mod tests {
    use crate::identity::KeyPair;
    use crate::secret_group::mls::MlsProvider;

    use super::MlsMember;

    #[test]
    fn public_key_identity() {
        let key_pair = KeyPair::new();
        let public_key_bytes = key_pair.public_key().to_bytes();
        let provider = MlsProvider::new();
        let member = MlsMember::new(&provider, &key_pair);
        assert_eq!(public_key_bytes.to_vec(), member.credential().identity());
    }
}
