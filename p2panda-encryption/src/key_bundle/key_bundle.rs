// SPDX-License-Identifier: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::x25519::PublicKey;
use crate::crypto::xeddsa::{XEdDSAError, XSignature, xeddsa_verify};
use crate::key_bundle::{LifetimeError, OneTimePreKey, OneTimePreKeyId, PreKey};
use crate::traits::KeyBundle;

/// Key-bundle with public keys to be used exactly _once_.
///
/// Note that while pre-keys are signed for X3DH, bundles should be part of an authenticated
/// messaging format where the whole payload (and thus it's lifetime and one-time pre-key) is
/// signed by the same identity to prevent replay and impersonation attacks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OneTimeKeyBundle {
    identity_key: PublicKey,
    signed_prekey: PreKey,
    prekey_signature: XSignature,
    onetime_prekey: Option<OneTimePreKey>,
}

impl OneTimeKeyBundle {
    pub fn new(
        identity_key: PublicKey,
        signed_prekey: PreKey,
        prekey_signature: XSignature,
        onetime_prekey: Option<OneTimePreKey>,
    ) -> Self {
        Self {
            identity_key,
            signed_prekey,
            prekey_signature,
            onetime_prekey,
        }
    }
}

impl KeyBundle for OneTimeKeyBundle {
    fn identity_key(&self) -> &PublicKey {
        &self.identity_key
    }

    fn signed_prekey(&self) -> &PublicKey {
        self.signed_prekey.key()
    }

    fn onetime_prekey(&self) -> Option<&PublicKey> {
        self.onetime_prekey.as_ref().map(|key| key.key())
    }

    fn onetime_prekey_id(&self) -> Option<OneTimePreKeyId> {
        self.onetime_prekey.as_ref().map(|key| key.id())
    }

    fn verify(&self) -> Result<(), KeyBundleError> {
        // Check lifetime.
        self.signed_prekey.verify_lifetime()?;

        // Check signature.
        xeddsa_verify(
            self.signed_prekey.as_bytes(),
            &self.identity_key,
            &self.prekey_signature,
        )?;

        Ok(())
    }
}

/// Key-bundle with public keys to be used until the pre-key expired.
///
/// Note that while pre-keys are signed for X3DH, bundles should be part of an authenticated
/// messaging format where the whole payload (and thus it's lifetime) is signed by the same
/// identity to prevent replay and impersonation attacks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LongTermKeyBundle {
    identity_key: PublicKey,
    signed_prekey: PreKey,
    prekey_signature: XSignature,
}

impl LongTermKeyBundle {
    pub fn new(
        identity_key: PublicKey,
        signed_prekey: PreKey,
        prekey_signature: XSignature,
    ) -> Self {
        Self {
            identity_key,
            signed_prekey,
            prekey_signature,
        }
    }
}

impl KeyBundle for LongTermKeyBundle {
    fn identity_key(&self) -> &PublicKey {
        &self.identity_key
    }

    fn signed_prekey(&self) -> &PublicKey {
        self.signed_prekey.key()
    }

    fn onetime_prekey(&self) -> Option<&PublicKey> {
        // No one-time pre-key in long-term key bundle.
        None
    }

    fn onetime_prekey_id(&self) -> Option<OneTimePreKeyId> {
        // No one-time pre-key in long-term key bundle.
        None
    }

    fn verify(&self) -> Result<(), KeyBundleError> {
        // Check lifetime.
        self.signed_prekey.verify_lifetime()?;

        // Check signature.
        xeddsa_verify(
            self.signed_prekey.as_bytes(),
            &self.identity_key,
            &self.prekey_signature,
        )?;

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum KeyBundleError {
    #[error(transparent)]
    XEdDSA(#[from] XEdDSAError),

    #[error(transparent)]
    Lifetime(#[from] LifetimeError),
}

#[cfg(test)]
mod tests {
    use crate::crypto::Rng;
    use crate::crypto::x25519::SecretKey;
    use crate::crypto::xeddsa::xeddsa_sign;
    use crate::key_bundle::{Lifetime, LongTermKeyBundle, OneTimePreKey, PreKey};
    use crate::traits::KeyBundle;

    use super::OneTimeKeyBundle;

    #[test]
    fn verify() {
        let rng = Rng::from_seed([1; 32]);

        let secret_key = SecretKey::from_bytes(rng.random_array().unwrap());
        let identity_key = secret_key.public_key().unwrap();

        let signed_prekey_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let signed_prekey = PreKey::new(
            signed_prekey_secret.public_key().unwrap(),
            Lifetime::default(),
        );
        let prekey_signature = xeddsa_sign(signed_prekey.as_bytes(), &secret_key, &rng).unwrap();

        let onetime_prekey_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let onetime_prekey = OneTimePreKey::new(onetime_prekey_secret.public_key().unwrap(), 1);

        // Valid key-bundles.
        assert!(
            OneTimeKeyBundle::new(
                identity_key,
                signed_prekey,
                prekey_signature,
                Some(onetime_prekey.clone()),
            )
            .verify()
            .is_ok()
        );
        assert!(
            LongTermKeyBundle::new(identity_key, signed_prekey, prekey_signature)
                .verify()
                .is_ok()
        );

        // Invalid lifetime of pre-key.
        let signed_prekey = PreKey::new(
            signed_prekey_secret.public_key().unwrap(),
            Lifetime::from_range(0, 0),
        );
        assert!(
            OneTimeKeyBundle::new(
                identity_key,
                signed_prekey,
                prekey_signature,
                Some(onetime_prekey.clone()),
            )
            .verify()
            .is_err()
        );
        assert!(
            LongTermKeyBundle::new(identity_key, signed_prekey, prekey_signature)
                .verify()
                .is_err()
        );

        // Invalid signature of pre-key.
        let prekey_signature = xeddsa_sign(b"wrong payload", &secret_key, &rng).unwrap();
        assert!(
            OneTimeKeyBundle::new(
                identity_key,
                signed_prekey,
                prekey_signature,
                Some(onetime_prekey.clone()),
            )
            .verify()
            .is_err()
        );
        assert!(
            LongTermKeyBundle::new(identity_key, signed_prekey, prekey_signature)
                .verify()
                .is_err()
        );
    }
}
