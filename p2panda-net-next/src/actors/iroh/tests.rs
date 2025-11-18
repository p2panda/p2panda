// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use iroh::discovery::UserData;
use iroh::protocol::ProtocolHandler;
use p2panda_core::PrivateKey;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{call, cast};
use tokio::time::sleep;

use crate::actors::address_book::{ADDRESS_BOOK, AddressBook, ToAddressBook};
use crate::actors::iroh::{IrohEndpoint, Mdns, ToIrohEndpoint, UserDataTransportInfo};
use crate::actors::{generate_actor_namespace, with_namespace};
use crate::test_utils::{setup_logging, test_args_from_seed};
use crate::utils::from_public_key;
use crate::{MdnsDiscoveryMode, TransportInfo};

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

    let (args_alice, _, _) = test_args_from_seed([1; 32]);
    let (args_bob, _, _) = test_args_from_seed([2; 32]);

    let thread_pool = ThreadLocalActorSpawner::new();

    // Spawn both endpoint actors.
    let (alice_ref, _) = IrohEndpoint::spawn(None, args_alice.clone(), thread_pool.clone())
        .await
        .expect("actor spawns successfully");
    let (bob_ref, _) = IrohEndpoint::spawn(None, args_bob.clone(), thread_pool.clone())
        .await
        .expect("actor spawns successfully");

    // Wait for endpoints to bind.
    sleep(Duration::from_millis(50)).await;

    // Alice registers the "echo" protocol to accept incoming connections for it.
    cast!(
        alice_ref,
        ToIrohEndpoint::RegisterProtocol(ECHO_PROTOCOL_ID.to_vec(), Box::new(EchoProtocol))
    )
    .expect("calling actor should not fail");

    // Create an iroh endpoint address of Alice, so Bob can connect.
    let alice_addr = iroh::EndpointAddr::new(from_public_key(args_alice.public_key)).with_ip_addr(
        (
            args_alice.iroh_config.bind_ip_v4,
            args_alice.iroh_config.bind_port_v4,
        )
            .into(),
    );

    // Bob connects to Alice using the "echo" protocol.
    let connection = call!(
        bob_ref,
        ToIrohEndpoint::Connect,
        alice_addr,
        ECHO_PROTOCOL_ID.to_vec()
    )
    .expect("calling actor should not fail")
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
    bob_ref.stop(None);
    alice_ref.stop(None);
}

#[tokio::test]
async fn mdns_discovery() {
    setup_logging();

    let (mut args_alice, store_alice, _) = test_args_from_seed([100; 32]);
    let (mut args_bob, store_bob, _) = test_args_from_seed([200; 32]);

    // Enable active discovery mode, otherwise they'll not find each other.
    args_alice.iroh_config.mdns_discovery_mode = MdnsDiscoveryMode::Active;
    args_bob.iroh_config.mdns_discovery_mode = MdnsDiscoveryMode::Active;

    let alice_namespace = generate_actor_namespace(&args_alice.public_key);
    let bob_namespace = generate_actor_namespace(&args_bob.public_key);

    let thread_pool = ThreadLocalActorSpawner::new();

    // Spawn address book (it's a dependency) for both.
    let (address_book_alice_ref, _) = AddressBook::spawn(
        Some(with_namespace(ADDRESS_BOOK, &alice_namespace)),
        (store_alice,),
        thread_pool.clone(),
    )
    .await
    .unwrap();
    let (address_book_bob_ref, _) = AddressBook::spawn(
        Some(with_namespace(ADDRESS_BOOK, &bob_namespace)),
        (store_bob,),
        thread_pool.clone(),
    )
    .await
    .unwrap();

    // Spawn mdns services for both.
    let (alice_ref, _) = Mdns::spawn(None, args_alice.clone(), thread_pool.clone())
        .await
        .unwrap();
    let (bob_ref, _) = Mdns::spawn(None, args_bob.clone(), thread_pool.clone())
        .await
        .unwrap();

    // Spawn both endpoint actors, it will populate the address books with the address info.
    let (endpoint_alice_ref, _) =
        IrohEndpoint::spawn(None, args_alice.clone(), thread_pool.clone())
            .await
            .expect("actor spawns successfully");
    let (endpoint_bob_ref, _) = IrohEndpoint::spawn(None, args_bob.clone(), thread_pool.clone())
        .await
        .expect("actor spawns successfully");

    // Wait for endpoints to bind.
    sleep(Duration::from_millis(50)).await;

    // Wait until they find each other and exchange transport infos.
    sleep(Duration::from_millis(1000)).await;

    // Alice should be in Bob's address book and vice-versa.
    let result = call!(
        address_book_bob_ref,
        ToAddressBook::NodeInfo,
        args_alice.public_key
    )
    .unwrap();
    assert!(result.is_some());

    let result = call!(
        address_book_alice_ref,
        ToAddressBook::NodeInfo,
        args_bob.public_key
    )
    .unwrap();
    assert!(result.is_some());

    // Shut down all actors since they're not supervised.
    address_book_alice_ref.stop(None);
    address_book_bob_ref.stop(None);
    bob_ref.stop(None);
    alice_ref.stop(None);
    endpoint_alice_ref.stop(None);
    endpoint_bob_ref.stop(None);
}

#[test]
fn transport_info_to_user_data() {
    // Create simple transport info object without any addresses attached.
    let private_key = PrivateKey::new();
    let transport_info = TransportInfo::new_unsigned().sign(&private_key).unwrap();

    // Extract information we want for our TXT record.
    let txt_info = UserDataTransportInfo::from_transport_info(transport_info);

    // Convert it into iroh data type.
    let user_data = UserData::try_from(txt_info.clone()).unwrap();

    // .. and back!
    let txt_info_again = UserDataTransportInfo::try_from(user_data).unwrap();
    assert_eq!(txt_info, txt_info_again);
}
