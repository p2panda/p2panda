// SPDX-License-Identifier: MIT OR Apache-2.0

use iroh::EndpointAddr;
use iroh::endpoint::{AfterHandshakeOutcome, BeforeConnectOutcome, Connection, EndpointHooks};
use iroh::protocol::ProtocolHandler;
use p2panda_core::VerifyingKey;
use p2panda_core::test_utils::setup_logging;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::address_book::AddressBook;
use crate::iroh_endpoint::{Builder, Endpoint};
use crate::test_utils::test_args;
use crate::utils::to_verifying_key;

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
    let (_, _, _, relay_map, _, _) = Builder::new(address_book)
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
    let (_, _, _, relay_map, _, _) = Builder::new(address_book)
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

/// Test hook instance to report on outbound and inbound connections.
#[derive(Debug, Clone)]
struct TestHook {
    tx: UnboundedSender<MonitoredConnection>,
}

#[derive(Debug, PartialEq, Eq)]
enum MonitoredConnection {
    Outbound { remote: VerifyingKey },
    Inbound { remote: VerifyingKey },
}

/// Test implementation of the `EndpointHooks` trait.
///
/// This is a super minimal single connection reporting implementation.
impl EndpointHooks for TestHook {
    // Runs before an outgoing connection begins.
    async fn before_connect(
        &self,
        remote_addr: &EndpointAddr,
        _alpn: &[u8],
    ) -> BeforeConnectOutcome {
        self.tx
            .send(MonitoredConnection::Outbound {
                remote: to_verifying_key(remote_addr.id),
            })
            .ok();

        BeforeConnectOutcome::Accept
    }

    // Runs after the handshake for both incoming and outgoing connections.
    async fn after_handshake(&self, conn: &Connection) -> AfterHandshakeOutcome {
        if conn.side().is_server() {
            self.tx
                .send(MonitoredConnection::Inbound {
                    remote: to_verifying_key(conn.remote_id()),
                })
                .ok();
        }

        AfterHandshakeOutcome::Accept
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

    // Instantiate endpoint hooks for both.
    let (alice_hook_tx, mut alice_hook_rx) = unbounded_channel();
    let (bob_hook_tx, mut bob_hook_rx) = unbounded_channel();
    let alice_hook = TestHook { tx: alice_hook_tx };
    let bob_hook = TestHook { tx: bob_hook_tx };

    // Spawn both endpoint actors.
    let alice_endpoint = Endpoint::builder(alice_address_book)
        .config(alice_args.iroh_config.clone())
        .signing_key(alice_args.signing_key.clone())
        .hooks(alice_hook)
        .spawn()
        .await
        .unwrap();

    let bob_endpoint = Endpoint::builder(bob_address_book.clone())
        .config(bob_args.iroh_config.clone())
        .signing_key(bob_args.signing_key.clone())
        .hooks(bob_hook)
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

    // Bob should receive a notification of the outbound connection.
    if let Some(event) = bob_hook_rx.recv().await {
        assert_eq!(
            event,
            MonitoredConnection::Outbound {
                remote: alice_args.verifying_key
            }
        );
    }

    // Alice should receive a notification of the inbound connection.
    if let Some(event) = alice_hook_rx.recv().await {
        assert_eq!(
            event,
            MonitoredConnection::Inbound {
                remote: bob_args.verifying_key
            }
        );
    }

    // Shut down connection and actors.
    connection.close(0u32.into(), b"bye!");
}
