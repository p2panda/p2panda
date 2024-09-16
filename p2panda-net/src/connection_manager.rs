// Connection manager.
//
// Minimal functionality for first-pass:
//
// - Maintain an address book
//   - Update upon discovery of new peers
// - Connect to new peers
// - Handle inbound peer connections
// - Invoke sync sessions
// - Record successful sync sessions
//   - I think...
// - Disconnect cleanly
//
// Second-pass features:
//
// - Retry failed connection attempts
//   - Implement cool-down for recurrent failures
// - Ensure maximum concurrent connection limit is respected

use std::collections::HashMap;

use iroh_gossip::proto::TopicId;
use iroh_net::NodeId;

// @TODO: Look at `PeerMap` in `src/engine/engine.rs`
// That contains some address book functionality.
// Be sure we're not duplicating efforts.

struct ConnectionManager {
    address_book: HashMap<NodeId, TopicId>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            address_book: HashMap::new(),
        }
    }

    pub fn connect() {
        todo!()
    }

    pub fn disconnect() {
        todo!()
    }

    pub fn handle_connection() {
        todo!()
    }

    fn add_peer() {
        todo!()
    }

    fn remove_peer() {
        todo!()
    }
}
