// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;
use std::fmt;
use std::hash::Hash as StdHash;

use ed25519_dalek_v2::{Signature as Ed25519Signature, SIGNATURE_LENGTH};
use serde::{Deserialize, Serialize};

use crate::identity::error::SignatureError;

pub const SIGNATURE_SIZE: usize = SIGNATURE_LENGTH;

/// Ed25519 signature.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct Signature(Ed25519Signature);

impl Signature {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SignatureError> {
        // Check if length is correct
        let bytes: [u8; SIGNATURE_LENGTH] = bytes
            .try_into()
            .map_err(|_| SignatureError::InvalidLength)?;

        let signature = Ed25519Signature::from_bytes(&bytes);
        Ok(Self(signature))
    }

    /// Returns signature as bytes.
    pub fn into_bytes(&self) -> Vec<u8> {
        self.0.to_bytes().to_vec()
    }
}

impl From<&Ed25519Signature> for Signature {
    fn from(signature: &Ed25519Signature) -> Self {
        Self(signature.to_owned())
    }
}

impl From<&Signature> for Ed25519Signature {
    fn from(signature: &Signature) -> Self {
        signature.0.to_owned()
    }
}

impl StdHash for Signature {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.into_bytes().hash(state)
    }
}

impl PartialEq for Signature {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Signature {}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.into_bytes()))
    }
}
