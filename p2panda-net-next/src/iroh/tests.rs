// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use iroh::protocol::ProtocolHandler;
use tokio::time::sleep;

use crate::MdnsDiscoveryMode;
use crate::address_book::AddressBook;
use crate::iroh::Endpoint;
use crate::test_utils::{
    generate_trusted_node_info, setup_logging, test_args, test_args_from_seed,
};

const ECHO_PROTOCOL_ID: &[u8] = b"test/echo/v1";

#[derive(Debug)]
struct EchoProtocol;

impl ProtocolHandler for EchoProtocol {
    async fn accept(
        &self,
        connection: iroh::endpoint::Connection,
    ) -> Result<(), iroh::protocol::AcceptError> {
        let (mut tx, mut rx) = connection.accept_bi().await?;

        // Echo any bytes received back directly.
        let _bytes_sent = tokio::io::copy(&mut rx, &mut tx).await?;

        tx.finish()?;
        connection.closed().await;

        Ok(())
    }
}

#[tokio::test]
async fn establish_connection() {
    setup_logging();

    let (mut alice_args, _, _) = test_args();
    let (bob_args, _, _) = test_args();

    // Spawn address book (it's a dependency) for both.
    let alice_address_book = AddressBook::builder().spawn().await.unwrap();
    let bob_address_book = AddressBook::builder().spawn().await.unwrap();

    // Spawn both endpoint actors.
    let alice_endpoint = Endpoint::builder(alice_address_book)
        .config(alice_args.iroh_config.clone())
        .private_key(alice_args.private_key.clone())
        .spawn()
        .await
        .unwrap();

    let bob_endpoint = Endpoint::builder(bob_address_book.clone())
        .config(bob_args.iroh_config.clone())
        .private_key(bob_args.private_key.clone())
        .spawn()
        .await
        .unwrap();

    // Wait for endpoints to bind.
    sleep(Duration::from_millis(50)).await;

    // Alice registers the "echo" protocol to accept incoming connections for it.
    alice_endpoint
        .accept(ECHO_PROTOCOL_ID, EchoProtocol)
        .await
        .unwrap();

    // Register iroh endpoint address of Alice, so Bob can connect.
    bob_address_book
        .insert_node_info(generate_trusted_node_info(&mut alice_args))
        .await
        .unwrap();

    // Bob connects to Alice using the "echo" protocol.
    let connection = bob_endpoint
        .connect(alice_args.public_key, ECHO_PROTOCOL_ID)
        .await
        .expect("connection establishment should not fail");

    // Send something to Alice.
    let (mut tx, mut rx) = connection.open_bi().await.expect("establish bi-di stream");
    tx.write_all(b"i feel so").await.unwrap();
    tx.finish().unwrap();

    // Receive the echo.
    let response = rx.read_to_end(1000).await.unwrap();
    assert_eq!(&response, b"i feel so");

    // Shut down connection and actors.
    connection.close(0u32.into(), b"bye!");
}

#[ignore = "mdns need to be moved into own module first"]
#[tokio::test]
async fn mdns_discovery() {
    setup_logging();

    let (mut alice_args, _, _) = test_args_from_seed([100; 32]);
    let (mut bob_args, _, _) = test_args_from_seed([200; 32]);

    // Enable active discovery mode, otherwise they'll not find each other.
    alice_args.iroh_config.mdns_discovery_mode = MdnsDiscoveryMode::Active;
    bob_args.iroh_config.mdns_discovery_mode = MdnsDiscoveryMode::Active;

    // Spawn address book (it's a dependency) for both.
    let alice_address_book = AddressBook::builder().spawn().await.unwrap();
    let bob_address_book = AddressBook::builder().spawn().await.unwrap();

    // Spawn both endpoint actors, it will populate the address books with the address info.
    let _alice_endpoint = Endpoint::builder(alice_address_book.clone())
        .config(alice_args.iroh_config.clone())
        .private_key(alice_args.private_key.clone())
        .spawn()
        .await
        .unwrap();

    let _bob_endpoint = Endpoint::builder(bob_address_book.clone())
        .config(bob_args.iroh_config.clone())
        .private_key(bob_args.private_key.clone())
        .spawn()
        .await
        .unwrap();

    // Wait until they find each other and exchange transport infos.
    sleep(Duration::from_millis(1000)).await;

    // Alice should be in Bob's address book and vice-versa.
    let result = bob_address_book
        .node_info(alice_args.public_key)
        .await
        .unwrap();
    assert!(result.is_some());

    let result = alice_address_book
        .node_info(bob_args.public_key)
        .await
        .unwrap();
    assert!(result.is_some());
}
