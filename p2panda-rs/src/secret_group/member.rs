// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::prelude::{Credential, KeyPackage};
use openmls_traits::OpenMlsCryptoProvider;

use crate::identity::KeyPair;
use crate::secret_group::mls::MlsMember;
use crate::secret_group::SecretGroupError;

/// Member of a secret group.
#[derive(Debug, Clone)]
pub struct SecretGroupMember {
    mls_member: MlsMember,
}

impl SecretGroupMember {
    /// Creates a new secret group member based on a p2panda key pair.
    pub fn new(
        provider: &impl OpenMlsCryptoProvider,
        key_pair: &KeyPair,
    ) -> Result<Self, SecretGroupError> {
        let mls_member = MlsMember::new(provider, key_pair)?;
        Ok(SecretGroupMember { mls_member })
    }

    /// Generates a new KeyPackage which can be used by others to invite this member into their
    /// groups.
    pub fn key_package(
        &self,
        provider: &impl OpenMlsCryptoProvider,
    ) -> Result<KeyPackage, SecretGroupError> {
        Ok(self.mls_member.key_package(provider)?)
    }

    /// Returns the MLS Credential of this group member.
    pub fn credential(&self) -> &Credential {
        self.mls_member.credential()
    }
}

#[cfg(test)]
mod tests {
    use openmls::prelude::{CredentialBundle, KeyPackageBundle};
    use openmls_traits::key_store::OpenMlsKeyStore;
    use openmls_traits::OpenMlsCryptoProvider;

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
        let key_package_bundle: Option<KeyPackageBundle> =
            provider.key_store().read(&key_package.hash(&provider));
        let credential_bundle: Option<CredentialBundle> = provider
            .key_store()
            .read(&member.credential().signature_key());
        assert!(key_package_bundle.is_some());
        assert!(credential_bundle.is_some());
    }
}
