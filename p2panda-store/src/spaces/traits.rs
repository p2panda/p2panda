// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use crate::spaces::SpacesMessage;

pub trait SpacesMessageStore<ID, A> {
    type Error: Error;

    fn get_spaces_message(
        &self,
        id: &ID,
    ) -> impl Future<Output = Result<Option<SpacesMessage<A>>, Self::Error>>;
}

pub trait SpacesStore<ID, S> {
    type Error: Error;

    fn get_space_state_tx(&self, id: &ID) -> impl Future<Output = Result<Option<S>, Self::Error>>;

    fn has_space(&self, id: &ID) -> impl Future<Output = Result<bool, Self::Error>>;

    fn space_ids(&self) -> impl Future<Output = Result<Vec<ID>, Self::Error>>;
}

pub trait SpacesStoreWrite<ID, S> {
    type Error: Error;

    fn set_space_state_tx(&self, id: &ID, y: &S) -> impl Future<Output = Result<(), Self::Error>>;
}
