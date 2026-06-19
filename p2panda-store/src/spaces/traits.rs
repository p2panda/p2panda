// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use p2panda_core::Hash;

use crate::spaces::SpacesMessage;

pub trait SpacesMessageStore<ARG> {
    type Error: Error;

    fn get_spaces_message(
        &self,
        id: &Hash,
    ) -> impl Future<Output = Result<Option<SpacesMessage<ARG>>, Self::Error>>;
}

pub trait SpacesStore<S> {
    type Error: Error;

    fn get_space_state_tx(&self, id: &Hash)
    -> impl Future<Output = Result<Option<S>, Self::Error>>;

    fn set_space_state_tx(&self, id: &Hash, y: &S)
    -> impl Future<Output = Result<(), Self::Error>>;

    fn has_space(&self, id: &Hash) -> impl Future<Output = Result<bool, Self::Error>>;

    fn space_ids(&self) -> impl Future<Output = Result<Vec<Hash>, Self::Error>>;
}
