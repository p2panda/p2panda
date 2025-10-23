// SPDX-License-Identifier: MIT OR Apache-2.0

pub struct AddressBook<S> {
    store: S,
}

impl<S> AddressBook<S>
where
    S: AddressBookStore,
{
    pub fn new(store: S) -> Self {
        Self { store }
    }
}

// @TODO: Move this into `p2panda-store` when it's ready.
pub trait AddressBookStore {}

#[cfg(test)]
mod tests {
    use super::{AddressBook, AddressBookStore};

    #[derive(Debug, Default)]
    struct TestStore {}

    impl AddressBookStore for TestStore {}

    #[test]
    fn it_works() {
        let store = TestStore::default();
        let address_book = AddressBook::new(store);
    }
}
