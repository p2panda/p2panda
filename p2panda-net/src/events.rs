//! System events API.

use iroh_net::key::PublicKey;

#[derive(Debug, Clone)]
pub enum SystemEvent<T> {
    GossipJoined {
        topic_id: [u8; 32],
        peers: Vec<PublicKey>,
    },
    GossipLeft {
        topic_id: [u8; 32],
    },
    GossipNeighborUp {
        topic_id: [u8; 32],
        peer: PublicKey,
    },
    GossipNeighborDown {
        topic_id: [u8; 32],
        peer: PublicKey,
    },
    SyncDone {
        topic: T,
        peer: PublicKey,
    },
}

/*
#[derive(Debug)]
pub struct SystemState<T> {
    events: Receiver<SystemEvent<T>>,
    completed_sync_sessions: HashMap<T, u16>,
    gossip_peers: HashMap<[u8; 32], u16>,
}

impl SystemState {
    pub fn new(events: Receiver<SystemEvent>) -> Self {
        Self {
            events,
            completed_sync_sessions: 0,
            gossip_peers: HashMap::new(),
        }
    }

    pub fn run(&mut self) -> Result<()> {
        // Process events by passing them off to event handlers.
        todo!();
    }

    fn on_sync_done(&mut self, topic: T) {
        // Increment the counter for the given topic.
        todo!();
    }
}
*/
