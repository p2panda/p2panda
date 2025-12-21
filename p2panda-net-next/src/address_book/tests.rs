// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::address_book::AddressBook;
use crate::test_utils::test_args;

#[tokio::test]
async fn spawn() {
    let (args, store, _) = test_args();
    let _address_book = AddressBook::builder(args.public_key, store).spawn().await;
}
