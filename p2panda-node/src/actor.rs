// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_client::Query;
use p2panda_net::Network;
use tokio::sync::mpsc;

pub enum ToNodeActor {}

pub struct NodeActor {
    network: Network<Query>,
    inbox: mpsc::Receiver<ToNodeActor>,
}

impl NodeActor {
    pub fn new(network: Network<Query>, inbox: mpsc::Receiver<ToNodeActor>) -> Self {
        Self { network, inbox }
    }

    pub async fn run(mut self) {
        self.run_inner().await;
    }

    async fn run_inner(&mut self) {
        loop {
            tokio::select! {
                biased;
                Some(_msg) = self.inbox.recv() => {
                    // @TODO
                }
            }
        }
    }
}
