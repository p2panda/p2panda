// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::credentials::{Credential, CredentialBundle, CredentialType};
use openmls::extensions::{Extension, LifetimeExtension};
use openmls::key_packages::{KeyPackage, KeyPackageBundle};
use openmls::prelude::SignatureScheme;
use openmls_traits::key_store::OpenMlsKeyStore;
use openmls_traits::OpenMlsCryptoProvider;

use crate::identity::KeyPair;
use crate::secret_group::mls::{MlsError, MLS_CIPHERSUITE_NAME, MLS_LIFETIME_EXTENSION_DAYS};

/// Wrapper around the MLS [CredentialBundle] of `openmls`.
#[derive(Debug, Clone)]
pub struct MlsMember {
    credential_bundle: CredentialBundle,
}

impl MlsMember {
    /// Creates a new MLS group member with a [`CredentialBundle`] using the p2panda [`KeyPair`] to
    /// authenticate the member of a group towards others.
    ///
    /// The generated bundle is automatically stored in the MLS key store.
    pub fn new(
        provider: &impl OpenMlsCryptoProvider,
        key_pair: &KeyPair,
    ) -> Result<Self, MlsError> {
        // Credential identities are p2panda public keys!
        let public_key = key_pair.public_key().to_bytes();

        // Check if CredentialBundle already exists in store, otherwise generate it
        let credential_bundle = match provider.key_store().read(&public_key) {
            None => {
                // @TODO: Not sure how this is possible to access ..
                /* // Full key here because we need it to sign
                let private_key = key_pair.private_key().to_bytes();
                let full_key = [private_key, public_key].concat();

                let signature_key_pair = SignatureKeypair::from_bytes(
                    MLS_CIPHERSUITE_NAME.into(),
                    full_key.to_vec(),
                    public_key.to_vec(),
                );

                // A CredentialBundle contains a Credential and the corresponding private key.
                // BasicCredential is a raw, unauthenticated assertion of an identity/key binding.
                let bundle = CredentialBundle::from_parts(public_key.to_vec(), signature_key_pair);

                // Persist CredentialBundle in key store for the future
                provider
                    .key_store()
                    .store(bundle.credential().signature_key(), &bundle)
                    .map_err(|_| MlsError::KeyStoreSerialization)?; */

                // @TODO: Remove this as soon as we figured out how to derive a `CredentialBundle`
                // from the p2panda key pair
                let bundle = CredentialBundle::new(
                    public_key.to_vec(),
                    CredentialType::Basic,
                    SignatureScheme::ED25519,
                    provider,
                )?;

                // Persist CredentialBundle in key store for the future
                provider
                    .key_store()
                    .store(bundle.credential().identity(), &bundle)
                    .map_err(|_| MlsError::KeyStoreSerialization)?;

                bundle
            }
            Some(bundle) => bundle,
        };

        Ok(Self { credential_bundle })
    }

    /// Returns [`Credential`] of this group member which is used to identify it.
    pub fn credential(&self) -> &Credential {
        self.credential_bundle.credential()
    }

    /// Generates a new [`KeyPackage`] of this group member and returns it.
    ///
    /// A [`KeyPackage`] object specifies a ciphersuite that the client supports, as well as
    /// providing a public key that others can use for key agreement.
    ///
    /// The generated [`KeyPackage`] is automatically stored inside the MLS key store.
    pub fn key_package(
        &self,
        provider: &impl OpenMlsCryptoProvider,
    ) -> Result<KeyPackage, MlsError> {
        // The lifetime extension represents the times between which clients will consider a
        // KeyPackage valid. Its use is mandatory in the MLS specification
        let lifetime_extension =
            Extension::LifeTime(LifetimeExtension::new(MLS_LIFETIME_EXTENSION_DAYS));

        // KeyPackageBundles contain KeyPackage with the corresponding private HPKE (Hybrid Public
        // Key Encryption) key.
        let key_package_bundle = KeyPackageBundle::new(
            &[MLS_CIPHERSUITE_NAME],
            &self.credential_bundle,
            provider,
            vec![lifetime_extension],
        )?;

        // Retrieve [KeyPackage] from bundle which is the public part of it
        let key_package = key_package_bundle.key_package().clone();
        let key_package_hash = key_package.hash_ref(provider.crypto())?.as_slice();

        // Save generated bundle in key-store
        provider
            .key_store()
            .store(&key_package_hash, &key_package_bundle)
            .map_err(|_| MlsError::KeyStoreSerialization)?;

        // Finally return the public part
        Ok(key_package)
    }
}

#[cfg(test)]
mod tests {
    use openmls::ciphersuite::signable::Verifiable;

    use crate::identity::KeyPair;
    use crate::secret_group::MlsProvider;

    use super::MlsMember;

    #[test]
    fn key_package_verify() {
        let key_pair = KeyPair::new();
        let provider = MlsProvider::new();
        let member = MlsMember::new(&provider, &key_pair).unwrap();
        let key_package = member.key_package(&provider).unwrap();

        assert!(key_package
            .verify_no_out(&provider, member.credential())
            .is_ok());
    }
}
