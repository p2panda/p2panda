// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::Signature;
use p2panda_core::cbor::encode_cbor;
use p2panda_encryption::Rng;
use p2panda_encryption::crypto::xeddsa::{XSignature, xeddsa_sign, xeddsa_verify};
use p2panda_encryption::key_bundle::{KeyBundleError, LongTermKeyBundle};
use p2panda_encryption::traits::KeyBundle;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Credentials, MemberId};

/// A group member and their associated long-term key bundle.
///
/// Authenticity between the verifying key and X3DH identity key is assured through cross-signing.
/// Through this we assure that the associated key bundle belongs to the verifying key as well.
/// Additional checks are performed against the key bundle's pre-key and it's lifetime.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// Verifying key of this space member, used as an identifier.
    id: MemberId,

    /// Associated X3DH key bundle with used identity key and pre-key.
    ///
    /// The pre-key's integrity and authenticity is assured by an xEdDSA signature inside the key
    /// bundle.
    ///
    /// ```text
    /// [Identity Key] --> [Pre-Key]
    /// ```
    key_bundle: LongTermKeyBundle,

    /// Signature proves that the member id is authenticted by the X3DH identity key and this
    /// particular key bundle (identified by the pre-key).
    ///
    /// ```text
    /// [Identity Key] --> [Pre-Key + Member-Id]
    /// ```
    cross_signature_1: XSignature,

    /// Signature proves that the X3DH identity key and associated key-bundle is authentic (cross
    /// signed).
    ///
    /// ```text
    /// [Verifying Key] --> [Pre-Key + Identity Key]
    /// ```
    cross_signature_2: Signature,
}

impl Member {
    pub(crate) fn new(
        rng: &Rng,
        credentials: &Credentials,
        key_bundle: LongTermKeyBundle,
    ) -> Result<Self, MemberError> {
        let id = credentials.verifying_key();

        let cross_signature_1 = {
            let mut bytes = id.as_bytes().to_vec();
            // Include pre-key as a nonce to prevent re-play attacks across key bundles.
            bytes.extend_from_slice(key_bundle.signed_prekey().as_bytes());
            // XEdDSA scheme uses a random entropy source, this gives us something like a nonce /
            // unique signature across member instances even if the inputs were the same.
            xeddsa_sign(&bytes, &credentials.identity_secret, rng)?
        };

        let cross_signature_2 = {
            let bytes = encode_cbor(&key_bundle)?;
            credentials.signing_key.sign(&bytes)
        };

        Ok(Self {
            id,
            key_bundle,
            cross_signature_1,
            cross_signature_2,
        })
    }

    /// Identifier for this member.
    pub fn id(&self) -> MemberId {
        self.id
    }

    /// Associated long-term key bundle for this member.
    pub fn key_bundle(&self) -> &LongTermKeyBundle {
        &self.key_bundle
    }

    /// Verify the key bundle and associated member id.
    pub fn verify(&self) -> Result<(), MemberError> {
        // Identity Key -> Pre-Key + Verifying Key.
        {
            let mut bytes = self.id.as_bytes().to_vec();
            bytes.extend_from_slice(self.key_bundle.signed_prekey().as_bytes());

            xeddsa_verify(
                &bytes,
                self.key_bundle.identity_key(),
                &self.cross_signature_1,
            )?;
        }

        // Verifying Key -> Identity Key.
        {
            let bytes = encode_cbor(&self.key_bundle)?;
            if !self.id.verify(&bytes, &self.cross_signature_2) {
                return Err(MemberError::MemberSignature);
            }
        }

        // Identity Key -> Pre-Key in key bundle & lifetime check.
        self.key_bundle.verify()?;

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum MemberError {
    #[error("failed encoding key bundle to compute or verify signature")]
    EncodeKeyBundle(#[from] p2panda_core::cbor::EncodeError),

    #[error("X3DH identity key could not prove that the member id is authentic")]
    IdentitySignature(#[from] p2panda_encryption::crypto::xeddsa::XEdDSAError),

    #[error("member could not prove that the X3DH identity key is authentic")]
    MemberSignature,

    #[error("key bundle is either expired or pre-key integrity invalid")]
    InvalidKeyBundle(#[from] KeyBundleError),
}

#[cfg(test)]
mod tests {
    use p2panda_encryption::Rng;
    use p2panda_encryption::key_bundle::{Lifetime, LongTermKeyBundle, PreKey};

    use crate::Credentials;

    use super::Member;

    #[test]
    fn verify_integrity() {
        let rng = Rng::from_seed([1; 32]);
        let credentials = Credentials::from_rng(&rng).unwrap();

        let key_bundle = {
            let prekey = PreKey::new(
                credentials.identity_secret().verifying_key().unwrap(),
                Lifetime::default(),
            );
            let signature = prekey.sign(&credentials.identity_secret(), &rng).unwrap();

            LongTermKeyBundle::new(
                credentials.identity_secret.verifying_key().unwrap(),
                prekey,
                signature,
            )
        };

        let member = Member::new(&rng, &credentials, key_bundle).unwrap();
        assert!(member.verify().is_ok());

        // Member id / verifying key is not matching signatures.
        let mut invalid_member = member.clone();
        invalid_member.id = {
            let credentials = Credentials::from_rng(&rng).unwrap();
            credentials.verifying_key()
        };
        assert!(invalid_member.verify().is_err());
    }

    #[test]
    fn replay_key_bundles() {
        let rng = Rng::from_seed([1; 32]);
        let credentials = Credentials::from_rng(&rng).unwrap();

        let key_bundle_1 = {
            let prekey = PreKey::new(
                credentials.identity_secret().verifying_key().unwrap(),
                Lifetime::default(),
            );
            let signature = prekey.sign(&credentials.identity_secret(), &rng).unwrap();

            LongTermKeyBundle::new(
                credentials.identity_secret.verifying_key().unwrap(),
                prekey,
                signature,
            )
        };

        let member_1 = Member::new(&rng, &credentials, key_bundle_1.clone()).unwrap();
        assert!(member_1.verify().is_ok());

        // Re-signing of the same key bundle using same credentials is okay, but will lead to
        // different signature (XEdDSA uses a random entropy source).
        let member_2 = Member::new(&rng, &credentials, key_bundle_1).unwrap();
        assert!(member_2.verify().is_ok());
        assert_ne!(member_1, member_2);

        // Re-use of the same signatures across key bundles is not permitted.
        let key_bundle_2 = {
            let prekey = PreKey::new(
                credentials.identity_secret().verifying_key().unwrap(),
                Lifetime::default(),
            );
            let signature = prekey.sign(&credentials.identity_secret(), &rng).unwrap();

            LongTermKeyBundle::new(
                credentials.identity_secret.verifying_key().unwrap(),
                prekey,
                signature,
            )
        };

        let member_3 = Member {
            id: member_1.id,
            key_bundle: key_bundle_2.clone(), // <--
            cross_signature_1: member_1.cross_signature_1,
            cross_signature_2: member_1.cross_signature_2,
        };
        assert!(member_3.verify().is_err());
        assert_eq!(member_1.id, member_2.id);
    }
}
