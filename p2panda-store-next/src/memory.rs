// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use rand::rand_core::UnwrapErr;
use rand::rngs::SysRng;

use crate::address_book::AddressBookMemoryStore;
use crate::operations::OperationMemoryStore;
use crate::orderer::OrdererMemoryStore;

/// In-memory store.
///
/// This does not persist data permamently, all changes are lost when the process ends. Use this
/// only in development or test contexts.
#[derive(Debug, Clone)]
pub struct MemoryStore<R, T, ID, N>
where
    T: Debug,
    ID: Debug,
{
    pub address_book: AddressBookMemoryStore<R, ID, N>,
    pub operations: OperationMemoryStore<T, ID>,
    pub orderer: OrdererMemoryStore<ID>,
}

impl<T, ID, N> MemoryStore<UnwrapErr<SysRng>, T, ID, N>
where
    T: Debug,
    ID: Debug,
{
    pub fn new() -> Self {
        Self::from_rng(UnwrapErr(SysRng))
    }
}

impl<R, T, ID, N> MemoryStore<R, T, ID, N>
where
    T: Debug,
    ID: Debug,
{
    pub fn from_rng(rng: R) -> Self {
        Self {
            address_book: AddressBookMemoryStore::new(rng),
            operations: OperationMemoryStore::new(),
            orderer: OrdererMemoryStore::new(),
        }
    }
}

impl<T, ID, N> Default for MemoryStore<UnwrapErr<SysRng>, T, ID, N>
where
    T: Debug,
    ID: Debug,
{
    fn default() -> Self {
        Self::new()
    }
}

// Trait implementations are in the relevant modules, see for example `orderer` or `operation` etc.
