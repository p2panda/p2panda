// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::cbor::encode_cbor;
use p2panda_core::{Signature, VerifyingKey};
use p2panda_encryption::Rng;
use p2panda_encryption::crypto::x25519::PublicKey;
use p2panda_encryption::crypto::xeddsa::{XSignature, xeddsa_sign, xeddsa_verify};
use p2panda_encryption::key_bundle::{KeyBundleError, Lifetime, LongTermKeyBundle, PreKey};
use p2panda_encryption::traits::KeyBundle;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Credentials, MemberId};

/// Version of the byte encoding.
const VERSION: u8 = 1;

/// Size of an encoded authenticated member:
///
/// * version (1)
/// * member id / verifying key (32)
/// * identity key (32)
/// * pre-key (32)
/// * lifetime not_before & not_after (2x 8)
/// * signature (64)
/// * cross-signatures (2x 64)
pub const MEMBER_BYTES_LEN: usize = 1 + 32 + 32 + 32 + 8 + 8 + 64 + 64 + 64;

/// A group member and their associated long-term key bundle.
///
/// Authenticity between the verifying key and X3DH identity key is assured through cross-signing.
/// Through this we assure that the associated key bundle belongs to the verifying key as well.
/// Additional checks are performed against the key bundle's pre-key and it's lifetime.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticatedMember {
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

impl AuthenticatedMember {
    pub(crate) fn sign(
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

    /// Compact encoding of this authenticated member info, to share it via a QR code for example.
    pub fn to_bytes(&self) -> [u8; MEMBER_BYTES_LEN] {
        let lifetime = self.key_bundle.lifetime();

        let mut bytes = Vec::with_capacity(MEMBER_BYTES_LEN);
        bytes.push(VERSION);
        bytes.extend_from_slice(self.id.as_bytes());
        bytes.extend_from_slice(self.key_bundle.identity_key().as_bytes());
        bytes.extend_from_slice(self.key_bundle.signed_prekey().as_bytes());
        bytes.extend_from_slice(&lifetime.not_before().to_be_bytes());
        bytes.extend_from_slice(&lifetime.not_after().to_be_bytes());
        bytes.extend_from_slice(self.key_bundle.prekey_signature().as_bytes());
        bytes.extend_from_slice(self.cross_signature_1.as_bytes());
        bytes.extend_from_slice(&self.cross_signature_2.to_bytes());

        bytes.try_into().expect("correct member info bytes length")
    }

    /// Decode an authenticated member info from bytes.
    ///
    /// **Warning:** Make sure to verify this information after decoding.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MemberError> {
        if bytes.len() != MEMBER_BYTES_LEN {
            return Err(MemberError::InvalidBytes);
        }

        if bytes[0] != VERSION {
            return Err(MemberError::UnsupportedVersion(bytes[0], VERSION));
        }

        // Helper to take next n bytes from slice.
        fn take<const N: usize>(bytes: &mut &[u8]) -> [u8; N] {
            let (head, tail) = bytes.split_at(N);
            *bytes = tail;
            head.try_into().expect("checked length before")
        }

        let mut bytes = &bytes[1..];

        let id =
            VerifyingKey::from_bytes(&take(&mut bytes)).map_err(|_| MemberError::InvalidBytes)?;
        let identity_key = PublicKey::from_bytes(take(&mut bytes));
        let prekey = PublicKey::from_bytes(take(&mut bytes));
        let lifetime = Lifetime::from_range(
            u64::from_be_bytes(take(&mut bytes)),
            u64::from_be_bytes(take(&mut bytes)),
        );
        let prekey_signature = XSignature::from_bytes(take(&mut bytes));
        let cross_signature_1 = XSignature::from_bytes(take(&mut bytes));
        let cross_signature_2 = Signature::from_bytes(&take(&mut bytes));

        Ok(Self {
            id,
            key_bundle: LongTermKeyBundle::new(
                identity_key,
                PreKey::new(prekey, lifetime),
                prekey_signature,
            ),
            cross_signature_1,
            cross_signature_2,
        })
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

    #[error("could not decode given authenticated member info bytes")]
    InvalidBytes,

    #[error("unsupported encoding version, given: {0}, expected: {1}")]
    UnsupportedVersion(u8, u8),
}

#[cfg(test)]
mod tests {
    use p2panda_encryption::Rng;
    use p2panda_encryption::key_bundle::{Lifetime, LongTermKeyBundle, PreKey};

    use crate::Credentials;

    use super::AuthenticatedMember;

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

        let member = AuthenticatedMember::sign(&rng, &credentials, key_bundle).unwrap();
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
    fn encoding_decoding_roundtrip() {
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

        let member = AuthenticatedMember::sign(&rng, &credentials, key_bundle).unwrap();
        let bytes = member.to_bytes();

        let decoded = AuthenticatedMember::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, member);
        assert!(decoded.verify().is_ok());

        // Fails if byte length is not correct.
        assert!(AuthenticatedMember::from_bytes(&bytes[1..]).is_err());
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

        let member_1 = AuthenticatedMember::sign(&rng, &credentials, key_bundle_1.clone()).unwrap();
        assert!(member_1.verify().is_ok());

        // Re-signing of the same key bundle using same credentials is okay, but will lead to
        // different signature (XEdDSA uses a random entropy source).
        let member_2 = AuthenticatedMember::sign(&rng, &credentials, key_bundle_1).unwrap();
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

        let member_3 = AuthenticatedMember {
            id: member_1.id,
            key_bundle: key_bundle_2.clone(), // <--
            cross_signature_1: member_1.cross_signature_1,
            cross_signature_2: member_1.cross_signature_2,
        };
        assert!(member_3.verify().is_err());
        assert_eq!(member_1.id, member_2.id);
    }
}
