// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{PrivateKey, PublicKey};

pub trait Forge<M> {
    type Error;

    fn public_key(&self) -> PublicKey;

    fn forge(&self, args: ForgeArgs) -> Result<M, Self::Error>;

    fn forge_with(&self, private_key: PrivateKey, args: ForgeArgs) -> Result<M, Self::Error>;
}

pub struct ForgeArgs {}
