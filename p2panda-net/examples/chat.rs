use anyhow::{bail, Result};
use clap::Parser;
use p2panda_core::{Hash, PrivateKey, PublicKey, Signature};
use p2panda_discovery::mdns::LocalDiscovery;
use p2panda_net::network::{FromNetwork, ToNetwork};
use p2panda_net::{NetworkBuilder, TopicId};
use p2panda_sync::TopicQuery;
use rand::random;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

// Here we have two relay URLs:
//
// One is an iroh staging relay which should be running the latest iroh release version.
// The other is operated by the p2panda team and may not be running the latest release version.
const RELAY_URL: &str = "https://staging-euw1-1.relay.iroh.network/";
//const RELAY_URL: &str = "https://wasser.liebechaos.org/";

pub fn setup_logging() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env())
        .try_init()
        .ok();
}

#[derive(Parser)]
struct Args {
    // Specify a relay to be registered with the local network builder.
    //
    // This is required if you wish to connect to a non-local bootstrap node. The relay will
    // facilitate a connection with the other node(s); either a direct connection or a proxied
    // connection if that is not possible.
    #[arg(short = 'r', long)]
    use_relay: bool,

    // Supply the public key of another peer to use it as a "bootstrap node" for discovery over
    // the internet.
    //
    // If no value is supplied, peers can only be discovered over your local area network.
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
    let topic = ChatTopic::new("my_chat");

    let private_key = PrivateKey::new();

    // Configure the network.
    let mut network_builder =
        NetworkBuilder::new(network_id.into()).discovery(LocalDiscovery::new());

    if args.use_relay {
        println!("using relay: {}", RELAY_URL);
        network_builder = network_builder.relay(RELAY_URL.parse()?, false, 0);
    }

    if let Some(node_id) = args.bootstrap {
        network_builder = network_builder.direct_address(node_id, vec![], None);
    }

    let network = network_builder.build().await?;

    let (tx, mut rx, ready) = network.subscribe(topic).await?;

    tokio::task::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                FromNetwork::GossipMessage { bytes, .. } => {
                    match Message::decode_and_verify(&bytes) {
                        Ok(message) => {
                            print!("{}: {}", message.public_key, message.text);
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

    println!("your public key is: {}", private_key.public_key());

    println!(".. waiting for peers to join ..");
    let _ = ready.await;
    println!("found other peers, you're ready to chat!");

    let (line_tx, mut line_rx) = mpsc::channel(1);
    std::thread::spawn(move || input_loop(line_tx));

    while let Some(text) = line_rx.recv().await {
        let bytes = Message::sign_and_encode(&private_key, &text)?;
        tx.send(ToNetwork::Message { bytes }).await.ok();
    }

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
        // Sign text content
        let mut text_bytes: Vec<u8> = Vec::new();
        ciborium::ser::into_writer(text, &mut text_bytes)?;
        let signature = private_key.sign(&text_bytes);

        // Encode message
        let message = Message {
            // Make every message unique, as duplicates get ignored during gossip broadcast
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
        // Decode message
        let message: Self = ciborium::de::from_reader(bytes)?;

        // Verify signature
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
