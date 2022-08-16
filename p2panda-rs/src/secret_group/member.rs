// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::credentials::Credential;
use openmls::key_packages::KeyPackage;
use openmls_traits::OpenMlsCryptoProvider;

use crate::identity::KeyPair;
use crate::secret_group::mls::MlsMember;
use crate::secret_group::SecretGroupError;

/// Member of a secret group holding the key material for creating and signing new KeyPackages.
///
/// ## Example
///
/// ```
/// # extern crate p2panda_rs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # use std::convert::TryFrom;
/// # use p2panda_rs::identity::KeyPair;
/// # use p2panda_rs::secret_group::{SecretGroupMember, MlsProvider};
/// // Define provider for cryptographic methods and key storage
/// let provider = MlsProvider::new();
///
/// // Generate new Ed25519 key pair
/// let key_pair = KeyPair::new();
///
/// // Create new group member based on p2panda key pair
/// let member = SecretGroupMember::new(&provider, &key_pair)?;
///
/// // Generate new KeyPackage which can be used to join other groups
/// let key_package = member.key_package(&provider)?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct SecretGroupMember {
    mls_member: MlsMember,
}

impl SecretGroupMember {
    /// Creates a new secret group member based on a p2panda [`KeyPair`].
    ///
    /// The [`KeyPair`] is used to authenticate the secret group member and its generated
    /// [`KeyPackage`] towards others.
    pub fn new(
        provider: &impl OpenMlsCryptoProvider,
        key_pair: &KeyPair,
    ) -> Result<Self, SecretGroupError> {
        let mls_member = MlsMember::new(provider, key_pair)?;
        Ok(SecretGroupMember { mls_member })
    }

    /// Generates a new [`KeyPackage`] which can be used by others to invite this member into their
    /// groups.
    pub fn key_package(
        &self,
        provider: &impl OpenMlsCryptoProvider,
    ) -> Result<KeyPackage, SecretGroupError> {
        Ok(self.mls_member.key_package(provider)?)
    }

    /// Returns the MLS [`Credential`] of this group member to identify itself.
    pub fn credential(&self) -> &Credential {
        self.mls_member.credential()
    }
}

#[cfg(test)]
mod tests {
    use openmls::credentials::CredentialBundle;
    use openmls::key_packages::KeyPackageBundle;
    use openmls_traits::key_store::OpenMlsKeyStore;
    use openmls_traits::OpenMlsCryptoProvider;
    use tls_codec::Serialize;

    use crate::identity::KeyPair;
    use crate::secret_group::mls::MlsProvider;

    use super::SecretGroupMember;

    #[test]
    fn public_key_identity() {
        let provider = MlsProvider::new();

        // Create new member
        let key_pair = KeyPair::new();
        let public_key_bytes = key_pair.public_key().to_bytes().to_vec();
        let member = SecretGroupMember::new(&provider, &key_pair).unwrap();

        // p2panda public key should be MLS credential identity
        assert_eq!(public_key_bytes, member.credential().identity());

        // Generated key packages should refer to same public key
        let key_package = member.key_package(&provider).unwrap();
        assert_eq!(public_key_bytes, key_package.credential().identity());
    }

    #[test]
    fn storage() {
        let provider = MlsProvider::new();

        // Create new member
        let key_pair = KeyPair::new();
        let member = SecretGroupMember::new(&provider, &key_pair).unwrap();

        // Generate KeyPackage
        let key_package = member.key_package(&provider).unwrap();

        // Credential bundle and key package got saved in key store
        let key_package_bundle: Option<KeyPackageBundle> = provider
            .key_store()
            .read(key_package.hash_ref(provider.crypto()).unwrap().as_slice());
        let credential_bundle: Option<CredentialBundle> = provider.key_store().read(
            &member
                .credential()
                .signature_key()
                .tls_serialize_detached()
                .unwrap(),
        );
        assert!(key_package_bundle.is_some());
        assert!(credential_bundle.is_some());
    }
}
