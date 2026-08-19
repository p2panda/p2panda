// SPDX-License-Identifier: MIT OR Apache-2.0

use iroh::protocol::ProtocolHandler;
use p2panda_core::test_utils::setup_logging;

use crate::address_book::AddressBook;
use crate::iroh_endpoint::{Builder, Endpoint};
use crate::test_utils::test_args;

const ECHO_PROTOCOL_ID: &[u8] = b"test/echo/v1";

#[derive(Debug)]
struct EchoProtocol;

#[tokio::test]
async fn configure_authentication_per_relay() {
    const FIRST_AUTH_TOKEN: &str = "first-secret-token";
    const SECOND_AUTH_TOKEN: &str = "second-secret-token";

    let first_relay_url: iroh::RelayUrl = "https://first-relay.example.com".parse().unwrap();
    let second_relay_url: iroh::RelayUrl = "https://second-relay.example.com".parse().unwrap();
    let public_relay_url: iroh::RelayUrl = "https://public-relay.example.com".parse().unwrap();
    let address_book = AddressBook::builder().spawn().await.unwrap();
    let (_, _, _, relay_map, _) = Builder::new(address_book)
        .relay_url_with_token(first_relay_url.clone(), FIRST_AUTH_TOKEN)
        .relay_url_with_token(second_relay_url.clone(), SECOND_AUTH_TOKEN)
        .relay_url(public_relay_url.clone())
        .build_args();

    let first_relay_config = relay_map.get(&first_relay_url).unwrap();
    assert_eq!(
        first_relay_config.auth_token.as_deref(),
        Some(FIRST_AUTH_TOKEN)
    );

    let second_relay_config = relay_map.get(&second_relay_url).unwrap();
    assert_eq!(
        second_relay_config.auth_token.as_deref(),
        Some(SECOND_AUTH_TOKEN)
    );

    let public_relay_config = relay_map.get(&public_relay_url).unwrap();
    assert_eq!(public_relay_config.auth_token, None);
}

#[tokio::test]
async fn relay_authentication_is_optional() {
    let relay_url: iroh::RelayUrl = "https://relay.example.com".parse().unwrap();
    let address_book = AddressBook::builder().spawn().await.unwrap();
    let (_, _, _, relay_map, _) = Builder::new(address_book)
        .relay_url(relay_url.clone())
        .build_args();

    let relay_config = relay_map.get(&relay_url).unwrap();
    assert_eq!(relay_config.auth_token, None);
}

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

    let mut alice_args = test_args();
    let bob_args = test_args();

    // Spawn address book (it's a dependency) for both.
    let alice_address_book = AddressBook::builder().spawn().await.unwrap();
    let bob_address_book = AddressBook::builder().spawn().await.unwrap();

    // Spawn both endpoint actors.
    let alice_endpoint = Endpoint::builder(alice_address_book)
        .config(alice_args.iroh_config.clone())
        .signing_key(alice_args.signing_key.clone())
        .spawn()
        .await
        .unwrap();

    let bob_endpoint = Endpoint::builder(bob_address_book.clone())
        .config(bob_args.iroh_config.clone())
        .signing_key(bob_args.signing_key.clone())
        .spawn()
        .await
        .unwrap();

    // Alice registers the "echo" protocol to accept incoming connections for it.
    alice_endpoint
        .accept(ECHO_PROTOCOL_ID, EchoProtocol)
        .await
        .unwrap();

    // Register iroh endpoint address of Alice, so Bob can connect.
    bob_address_book
        .insert_node_info(alice_args.node_info())
        .await
        .unwrap();

    // Bob connects to Alice using the "echo" protocol.
    let connection = bob_endpoint
        .connect(alice_args.verifying_key, ECHO_PROTOCOL_ID)
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
