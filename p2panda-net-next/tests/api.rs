// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::PrivateKey;
use p2panda_net_next::address_book::AddressBook;
use p2panda_net_next::discovery::Discovery;
use p2panda_net_next::iroh_endpoint::Endpoint;

#[tokio::test]
async fn modular_api() {
    let private_key = PrivateKey::new();

    let address_book = AddressBook::builder().spawn().await.unwrap();

    let endpoint = Endpoint::builder(address_book.clone())
        .private_key(private_key)
        .network_id([42; 32])
        .spawn()
        .await
        .unwrap();

    let _discovery = Discovery::builder(address_book, endpoint)
        .spawn()
        .await
        .unwrap();
}
