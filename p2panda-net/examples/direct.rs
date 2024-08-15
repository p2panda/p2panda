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

    let network = NetworkBuilder::new(network_id).build().await?;
    println!("{:?}, {}", network.direct_addresses().await.unwrap(), network.node_id());

    tokio::signal::ctrl_c().await?;

    network.shutdown().await?;

    Ok(())
}
