// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::PrivateKey;
use p2panda_net_next::address_book::AddressBook;
use p2panda_net_next::iroh::Endpoint;

#[tokio::test]
async fn modular_api() {
    let private_key = PrivateKey::new();
    let public_key = private_key.public_key();

    let address_book = AddressBook::builder(public_key).spawn().await.unwrap();

    let _endpoint = Endpoint::builder(address_book)
        .private_key(private_key)
        .spawn()
        .await
        .unwrap();
}
