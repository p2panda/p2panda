// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::crypto::x25519::PublicKey;

/// Manages public identity keys of other members.
pub trait IdentityRegistry<ID, Y> {
    type Error: Error;

    fn identity_key(y: &Y, id: &ID) -> Result<Option<PublicKey>, Self::Error>;
}

/// Manages public key bundles of other members.
pub trait PreKeyRegistry<ID, KB> {
    type State: Debug + Serialize + for<'a> Deserialize<'a>;

    type Error: Error;

    fn key_bundle(y: Self::State, id: &ID) -> Result<(Self::State, Option<KB>), Self::Error>;
}
