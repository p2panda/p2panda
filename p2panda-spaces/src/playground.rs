// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::Cell;
use std::cell::Ref;
use std::cell::RefCell;
use std::convert::Infallible;
use std::marker::PhantomData;
use std::rc::Rc;

use p2panda_store::{Transaction, WritableStore};

use crate::store::MemoryStore;

#[derive(Clone)]
pub struct Wrapper<S> {
    inner: Rc<RefCell<WrapperInner<S>>>,
}

pub struct WrapperInner<S> {
    store: S,
}

impl<S> Wrapper<S>
where
    S: WritableStore,
{
    pub fn new(store: S) -> Self {
        Self {
            inner: Rc::new(RefCell::new(WrapperInner { store })),
        }
    }
}

pub struct WrapperTransaction<S> {
    _marker: PhantomData<S>,
}

impl<S> Transaction for WrapperTransaction<S>
where
    S: WritableStore,
{
    type Error = S::Error;

    async fn commit(self) -> Result<(), Self::Error> {
        todo!()
    }

    async fn rollback(self) -> Result<(), Self::Error> {
        todo!()
    }
}

impl<S> WritableStore for Wrapper<S>
where
    S: WritableStore,
{
    type Error = S::Error;

    type Transaction<'c> = WrapperTransaction<S>;

    async fn begin<'c>(&mut self) -> Result<Self::Transaction<'c>, Self::Error> {
        todo!()
    }
}

// ---

impl<S> IcecreamStore for Wrapper<S>
where
    S: IcecreamStore,
{
    async fn icecream(&self) -> Icecream {
        let inner = self.inner.borrow();
        inner.store.icecream().await
    }

    async fn set_icecream(&mut self, icecream: Icecream) {
        todo!()
    }
}

// ---

#[derive(Clone, Default)]
pub struct MemoryState {
    icecream: Icecream,
}

impl IcecreamStore for MemoryStore<MemoryState> {
    async fn icecream(&self) -> Icecream {
        let state = self.state.read().await;
        state.icecream.clone()
    }

    async fn set_icecream(&mut self, icecream: Icecream) {
        todo!()
    }
}

// ---

pub struct Manager<S> {
    store: S,
}

impl<S> Manager<S>
where
    S: IcecreamStore,
{
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub async fn icecream(&self) -> Icecream {
        self.store.icecream().await
    }

    pub async fn set_icecream(&mut self, icecream: Icecream) {
        self.store.set_icecream(icecream).await;
    }
}

// ---

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Icecream {
    #[default]
    Vanilla,
    Chocolate,
    Strawberry,
}

pub trait IcecreamStore {
    fn icecream(&self) -> impl Future<Output = Icecream>;

    fn set_icecream(&mut self, icecream: Icecream) -> impl Future<Output = ()>;
}

#[cfg(test)]
mod tests {
    use p2panda_store::{Transaction, WritableStore};

    use crate::store::MemoryStore;

    use super::{Icecream, Manager, MemoryState, Wrapper};

    #[tokio::test]
    async fn it_works() {
        let memory_store = MemoryStore::new(MemoryState::default());

        let mut store = Wrapper::new(memory_store);

        let mut manager = Manager::new(store.clone());
        assert_eq!(manager.icecream().await, Icecream::Vanilla);

        let tx = store.begin().await.unwrap();

        manager.set_icecream(Icecream::Strawberry).await;
        assert_eq!(manager.icecream().await, Icecream::Strawberry);

        tx.rollback().await.unwrap();
        assert_eq!(manager.icecream().await, Icecream::Vanilla);
    }
}
