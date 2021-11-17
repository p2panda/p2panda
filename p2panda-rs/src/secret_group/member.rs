// SPDX-License-Identifier: AGPL-3.0-or-later

use ed25519_dalek::PublicKey;
use openmls::prelude::KeyPackage;
use openmls_traits::OpenMlsCryptoProvider;

use crate::identity::KeyPair;
use crate::secret_group::mls::MlsMember;

pub struct SecretGroupMember {
    mls_member: MlsMember,
}

impl SecretGroupMember {
    pub fn new(provider: &impl OpenMlsCryptoProvider, public_key: &PublicKey) -> Self {
        let mls_member = MlsMember::new(provider, &public_key.to_bytes());

        SecretGroupMember { mls_member }
    }

    pub fn key_package(&self, provider: &impl OpenMlsCryptoProvider) -> KeyPackage {
        self.mls_member.key_package(provider)
    }
}
