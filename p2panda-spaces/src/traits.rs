// SPDX-License-Identifier: MIT OR Apache-2.0

pub trait Forge<M> {
    type Error;

    fn forge(&self, args: ForgeArgs) -> Result<M, Self::Error>;
}

pub struct ForgeArgs {}
