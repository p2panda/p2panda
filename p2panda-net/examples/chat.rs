use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use iroh::endpoint::Connecting;
use iroh::{NodeAddr, PublicKey};
use p2panda_core::{Hash, PrivateKey};
use p2panda_net::{NetworkBuilder, ProtocolHandler, TopicId};
use p2panda_sync::TopicQuery;
use serde::{Deserialize, Serialize};
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

pub const TEST_ALPN: &[u8] = b"/test-protocol/0";

// const RELAY_URL: &str = "https://staging-euw1-1.relay.iroh.network/";
const RELAY_URL: &str = "https://euw1-1.relay.iroh.network/";
// const RELAY_URL: &str = "https://wasser.liebechaos.org/";

pub fn setup_logging() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env())
        .try_init()
        .ok();
}

#[derive(Parser)]
struct Args {
    #[arg(short = 'b', long, value_name = "PUBLIC_KEY")]
    bootstrap: Option<PublicKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct ChatTopic(String, [u8; 32]);

impl ChatTopic {
    pub fn new(name: &str) -> Self {
        Self(name.to_owned(), *Hash::new(name).as_bytes())
    }
}

impl TopicQuery for ChatTopic {}

impl TopicId for ChatTopic {
    fn id(&self) -> [u8; 32] {
        self.1
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    let args = Args::parse();

    let network_id = Hash::new(b"p2panda_chat_example");

    let private_key = PrivateKey::new();

    let test_protocol = TestProtocol {};

    println!("your public key is: {}", private_key.public_key());

    let network_builder = NetworkBuilder::<ChatTopic>::new(network_id.into())
        .private_key(private_key)
        .protocol(TEST_ALPN, test_protocol)
        .relay(RELAY_URL.parse()?, false, 0);

    // if let Some(node_id) = args.bootstrap {
    //     network_builder = network_builder.direct_address(node_id, vec![], None);
    // }

    let network = network_builder.build().await?;

    if let Some(node_id) = args.bootstrap {
        let network = network.clone();
        tokio::task::spawn(async move {
            let addr = NodeAddr::new(PublicKey::from_bytes(node_id.as_bytes()).unwrap())
                .with_relay_url(RELAY_URL.parse().unwrap());

            let connection = network.endpoint().connect(addr, TEST_ALPN).await.unwrap();

            let (mut send, mut recv) = connection.open_bi().await.unwrap();
            send.write_all(b"Hello, world!").await.unwrap();
            send.finish().unwrap();

            let response = recv.read_to_end(1000).await.unwrap();
            assert_eq!(&response, b"Hello, world!");
        });
    }

    tokio::signal::ctrl_c().await?;

    network.shutdown().await?;

    Ok(())
}

pub type BoxedFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

#[derive(Debug)]
struct TestProtocol {}

impl ProtocolHandler for TestProtocol {
    fn accept(self: Arc<Self>, connecting: Connecting) -> BoxedFuture<Result<()>> {
        Box::pin(async move {
            let connection = connecting.await?;
            let (mut send, mut recv) = connection.accept_bi().await?;

            // Echo any bytes received back directly.
            let _bytes_sent = tokio::io::copy(&mut recv, &mut send).await?;

            send.finish()?;
            connection.closed().await;

            Ok(())
        })
    }
}
