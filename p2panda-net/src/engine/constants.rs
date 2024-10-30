// SPDX-License-Identifier: AGPL-3.0-or-later

/// Maximum size of random sample set when choosing peers to join gossip overlay.
///
/// The larger the number the less likely joining the gossip will fail as we get more chances to
/// establish connections. As soon as we've joined the gossip we will learn about more peers.
pub const JOIN_PEERS_SAMPLE_LEN: usize = 7;
