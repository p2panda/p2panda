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
