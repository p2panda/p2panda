use anyhow::{bail, Result};
use p2panda_core::{PrivateKey, PublicKey, Signature};
use p2panda_net::network::{InEvent, OutEvent};
use p2panda_net::{LocalDiscovery, NetworkBuilder};
use rand::random;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

pub fn setup_logging() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env())
        .try_init()
        .ok();
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    let network_id = [0; 32];
    let topic_id = [1; 32];

    let private_key = PrivateKey::new();

    let network = NetworkBuilder::new(network_id)
        .discovery(LocalDiscovery::new()?)
        .build()
        .await?;

    let (tx, mut rx) = network.subscribe(topic_id).await?;
    let (ready_tx, mut ready_rx) = mpsc::channel::<()>(1);

    tokio::task::spawn(async move {
        while let Ok(event) = rx.recv().await {
            match event {
                OutEvent::Ready => {
                    ready_tx.send(()).await.ok();
                }
                OutEvent::Message {
                    bytes,
                    delivered_from,
                } => match Message::decode_and_verify(&bytes) {
                    Ok(message) => {
                        print!("{}: {}", message.public_key, message.text);
                    }
                    Err(err) => {
                        eprintln!("invalid message from {delivered_from}: {err}");
                    }
                },
            }
        }
    });

    println!(".. waiting for peers to join ..");
    let _ = ready_rx.recv().await;
    println!("found other peers, you're ready to chat!");

    let (line_tx, mut line_rx) = mpsc::channel(1);
    std::thread::spawn(move || input_loop(line_tx));

    while let Some(text) = line_rx.recv().await {
        let bytes = Message::sign_and_encode(&private_key, &text)?;
        tx.send(InEvent::Message { bytes }).await.ok();
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
