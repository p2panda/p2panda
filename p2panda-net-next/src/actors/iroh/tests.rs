// SPDX-License-Identifier: MIT OR Apache-2.0

use iroh::protocol::ProtocolHandler;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{call, cast};

use crate::actors::iroh::{IrohEndpoint, ToIrohEndpoint};
use crate::test_utils::{setup_logging, test_args_from_seed};
use crate::utils::from_public_key;

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
        let bytes_sent = tokio::io::copy(&mut rx, &mut tx).await?;

        tx.finish()?;
        connection.closed().await;

        Ok(())
    }
}

#[tokio::test]
async fn establish_connection() {
    setup_logging();

    let (args_alice, _) = test_args_from_seed([1; 32]);
    let (args_bob, _) = test_args_from_seed([2; 32]);

    let thread_pool = ThreadLocalActorSpawner::new();

    // Spawn both endpoint actors.
    let (alice_ref, _) = IrohEndpoint::spawn(None, args_alice.clone(), thread_pool.clone())
        .await
        .expect("actor spawns successfully");
    let (bob_ref, _) = IrohEndpoint::spawn(None, args_bob.clone(), thread_pool.clone())
        .await
        .expect("actor spawns successfully");

    // Alice registers the "echo" protocol to accept incoming connections for it.
    cast!(
        alice_ref,
        ToIrohEndpoint::RegisterProtocol(ECHO_PROTOCOL_ID.to_vec(), Box::new(EchoProtocol))
    )
    .expect("calling actor should not fail");

    // Create an iroh endpoint address of Alice, so Bob can connect to them.
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
}
