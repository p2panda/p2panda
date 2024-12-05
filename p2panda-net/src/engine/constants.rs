// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

/// Maximum size of random sample set when choosing peers to join gossip overlay.
///
/// The larger the number the less likely joining the gossip will fail as we get more chances to
/// establish connections. As soon as we've joined the gossip we will learn about more peers.
pub const JOIN_PEERS_SAMPLE_LEN: usize = 7;

/// Frequency of attempts to join the gossip overlay which is used for "topic discovery".
pub const JOIN_NETWORK_INTERVAL: Duration = Duration::from_millis(900);

/// Frequency of topic id announcements (to network peers).
pub const ANNOUNCE_TOPICS_INTERVAL: Duration = Duration::from_millis(2200);

/// Frequency of attempts to join gossip overlays for application-defined topic ids.
pub const JOIN_TOPICS_INTERVAL: Duration = Duration::from_millis(1200);
