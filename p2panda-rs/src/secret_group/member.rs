// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::prelude::KeyPackage;
use openmls_traits::OpenMlsCryptoProvider;

use crate::identity::KeyPair;
use crate::secret_group::mls::MlsMember;
use crate::secret_group::SecretGroupError;

#[derive(Debug, Clone)]
pub struct SecretGroupMember {
    mls_member: MlsMember,
}

impl SecretGroupMember {
    pub fn new(
        provider: &impl OpenMlsCryptoProvider,
        key_pair: &KeyPair,
    ) -> Result<Self, SecretGroupError> {
        let mls_member = MlsMember::new(provider, key_pair)?;
        Ok(SecretGroupMember { mls_member })
    }

    pub fn key_package(
        &self,
        provider: &impl OpenMlsCryptoProvider,
    ) -> Result<KeyPackage, SecretGroupError> {
        Ok(self.mls_member.key_package(provider)?)
    }
}
