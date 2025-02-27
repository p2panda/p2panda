//! Example chat program using `p2panda-net`.
//!
//! `cargo run --example chat -- --help`
//!
//! Arguments are exposed to control the networking setup, including local discovery over mDNS,
//! relay connectivity and the specification of a bootstrap peer.
//!
//! # Scenario 1: Local Connectivity
//!
//! Run the example with the `--use-mdns` flag if you wish to enable local network discovery.
//! Run the same command in a second terminal to chat over the local network.
//!
//! `cargo run --example chat -- --use-mdns`
//!
//! # Scenario 2: Internet Connectivity
//!
//! Run the example with the `--use-relay` flag; take note of the `node id` in the terminal output.
//!
//! `cargo run --example chat -- --use-relay --is-bootstrap`
//!
//! Run the example on a second computer or in a second terminal with the `--use-relay` and
//! `--bootstrap-peer <PUBLIC_KEY>` flags (passing in the `node id` from the first computer or
//! terminal as `PUBLIC_KEY`).
//!
//! `cargo run --example chat -- --use-relay --bootstrap-peer <PUBLIC_KEY>`
use anyhow::{Result, bail};
use clap::Parser;
use p2panda_core::{Hash, PrivateKey, PublicKey, Signature};
use p2panda_discovery::mdns::LocalDiscovery;
use p2panda_net::network::{FromNetwork, ToNetwork};
use p2panda_net::{NetworkBuilder, TopicId};
use p2panda_sync::TopicQuery;
use rand::random;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

// Relay server operated by p2panda team (may not be running the latest iroh release version).
const RELAY_URL: &str = "https://wasser.liebechaos.org/";

pub fn setup_logging() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env())
        .try_init()
        .ok();
}

#[derive(Parser)]
struct Args {
    /// Enable local discovery using mDNS.
    #[arg(short = 'm', long)]
    use_mdns: bool,

    /// Enable relay server connectivity.
    #[arg(short = 'r', long)]
    use_relay: bool,

    /// Enable bootstrap mode.
    #[arg(short = 'b', long)]
    is_bootstrap: bool,

    /// Supply the public key of a bootstrap peer for discovery over the internet.
    #[arg(short = 'p', long, value_name = "PUBLIC_KEY")]
    bootstrap_peer: Option<PublicKey>,
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
    let topic = ChatTopic::new("my_chat");

    let private_key = PrivateKey::new();
    let public_key = private_key.public_key();

    // Configure the network.
    let mut network_builder =
        NetworkBuilder::<ChatTopic>::new(network_id.into()).private_key(private_key.clone());

    if args.use_mdns {
        network_builder = network_builder.discovery(LocalDiscovery::new());
    }

    if args.use_relay {
        network_builder = network_builder.relay(RELAY_URL.parse()?, false, 3478);
    }

    if args.is_bootstrap {
        network_builder = network_builder.bootstrap();
    }

    if let Some(node_id) = args.bootstrap_peer {
        network_builder = network_builder.direct_address(node_id, vec![], None);
    }

    let network = network_builder.build().await?;

    // Print network info to the terminal.
    println!("node id:");
    println!("\t{}", public_key);
    println!("network id:");
    println!("\t{}", network_id);
    println!("node listening addresses:");
    for local_endpoint in network
        .endpoint()
        .direct_addresses()
        .initialized()
        .await
        .unwrap()
    {
        println!("\t{}", local_endpoint.addr)
    }
    println!("local discovery via mdns:");
    if args.use_mdns {
        println!("\tactive");
    } else {
        println!("\tinactive");
    }
    println!("node relay server url:");
    if args.use_relay {
        let relay_url = network
            .endpoint()
            .home_relay()
            .get()
            .unwrap()
            .expect("should be connected to a relay server");
        println!("\t{relay_url}");
    } else {
        println!("\tnot specified");
    }
    println!("bootstrap mode:");
    if args.is_bootstrap {
        println!("\tenabled");
    } else {
        println!("\tdisabled");
    }
    println!("bootstrap peer:");
    if let Some(node_id) = args.bootstrap_peer {
        println!("\t{node_id}");
    } else {
        println!("\tnot specified");
    }
    println!();

    // Subscribe to the chat topic.
    let (tx, mut rx, ready) = network.subscribe(topic).await?;

    // Receive topic messages from the network;
    // decode and verify their integrity before printing them to the terminal.
    tokio::task::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                FromNetwork::GossipMessage { bytes, .. } => {
                    match Message::decode_and_verify(&bytes) {
                        Ok(message) => {
                            print!("{}: {}", &message.public_key.to_string()[..5], message.text);
                        }
                        Err(err) => {
                            eprintln!("invalid gossip message: {err}");
                        }
                    }
                }
                _ => panic!("no sync messages expected"),
            }
        }
    });

    println!(".. waiting for peers to join ..");
    let _ = ready.await;
    println!("found other peers, you're ready to chat!");

    // Listen for text input via the terminal.
    let (line_tx, mut line_rx) = mpsc::channel(1);
    std::thread::spawn(move || input_loop(line_tx));

    // Sign and encode each line of text input and broadcast it on the chat topic.
    while let Some(text) = line_rx.recv().await {
        let bytes = Message::sign_and_encode(&private_key, &text)?;
        tx.send(ToNetwork::Message { bytes }).await.ok();
    }

    // Listen for `Ctrl+c` and shutdown the node.
    tokio::signal::ctrl_c().await?;
    network.shutdown().await?;

    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Message {
    id: u32,
    signature: Signature,
    public_key: PublicKey,
    text: String,
}

impl Message {
    pub fn sign_and_encode(private_key: &PrivateKey, text: &str) -> Result<Vec<u8>> {
        // Sign text content.
        let mut text_bytes: Vec<u8> = Vec::new();
        ciborium::ser::into_writer(text, &mut text_bytes)?;
        let signature = private_key.sign(&text_bytes);

        // Encode message.
        let message = Message {
            // Make every message unique, as duplicates get ignored during gossip broadcast.
            id: random(),
            signature,
            public_key: private_key.public_key(),
            text: text.to_owned(),
        };
        let mut bytes: Vec<u8> = Vec::new();
        ciborium::ser::into_writer(&message, &mut bytes)?;

        Ok(bytes)
    }

    fn decode_and_verify(bytes: &[u8]) -> Result<Self> {
        // Decode message.
        let message: Self = ciborium::de::from_reader(bytes)?;

        // Verify signature.
        let mut text_bytes: Vec<u8> = Vec::new();
        ciborium::ser::into_writer(&message.text, &mut text_bytes)?;
        if !message.public_key.verify(&text_bytes, &message.signature) {
            bail!("invalid signature");
        }

        Ok(message)
    }
}

fn input_loop(line_tx: mpsc::Sender<String>) -> Result<()> {
    let mut buffer = String::new();
    let stdin = std::io::stdin();
    loop {
        stdin.read_line(&mut buffer)?;
        line_tx.blocking_send(buffer.clone())?;
        buffer.clear();
    }
}
