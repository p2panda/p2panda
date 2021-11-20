// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::prelude::KeyPackage;
use openmls_traits::OpenMlsCryptoProvider;

use crate::identity::KeyPair;
use crate::secret_group::mls::MlsMember;

#[derive(Debug, Clone)]
pub struct SecretGroupMember {
    mls_member: MlsMember,
}

impl SecretGroupMember {
    // @TODO: Rename `Author` struct to `PublicKey`.
    pub fn new(provider: &impl OpenMlsCryptoProvider, key_pair: &KeyPair) -> Self {
        let mls_member = MlsMember::new(provider, key_pair).unwrap();
        SecretGroupMember { mls_member }
    }

    pub fn key_package(&self, provider: &impl OpenMlsCryptoProvider) -> KeyPackage {
        self.mls_member.key_package(provider).unwrap()
    }
}
