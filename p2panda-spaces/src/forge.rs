// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_core::{PrivateKey, PublicKey};

use crate::message::SpacesArgs;

pub trait Forge<ID, M, C> {
    type Error: Debug;

    fn public_key(&self) -> PublicKey;

    // @TODO: It is a little bit strange that the private key (author) is controlled by the forge
    // while the "identity key" (key agreement) is controlled by the "key manager" / owned by the
    // space manager.
    //
    // Since both keys should be rotated at once it would make sense to have an `Credentials`
    // object which holds both of them, to indicate in the API that they can't be changed without
    // changing the other.
    //
    // @TODO: Another thing is that we want to maybe detect key rotations as it means that people
    // will loose access to their spaces.
    fn forge(&mut self, args: SpacesArgs<ID, C>) -> impl Future<Output = Result<M, Self::Error>>;

    fn forge_ephemeral(
        &mut self,
        private_key: PrivateKey,
        args: SpacesArgs<ID, C>,
    ) -> impl Future<Output = Result<M, Self::Error>>;
}
