// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::hash::Hash as StdHash;

use crate::{Hash, PublicKey};

pub type SeqNum = u64;

/// State vector describing the heads of a log which may be forked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StateVector<ID = Hash>(HashSet<(ID, SeqNum)>)
where
    ID: PartialEq + Eq + StdHash;

impl StateVector {
    pub fn new() -> Self {
        Self(HashSet::new())
    }

    pub fn is_forked(&self) -> bool {
        self.len() > 1
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Compare one state vector with another and calculate the difference. This can be then used
    /// to understand which entries should be sent from one replica to another in order for them
    /// to converge to the same state.
    pub fn diff(&self, remote: &StateVector) -> Diff {
        let local = self;

        // The local and remote vectors are equal and so the entries they contain must be equal. No
        // further action is needed.
        if local == remote {
            return Diff::equal();
        }

        // The local is empty and so the remote must be ahead.
        if local.is_empty() {
            return Diff::ahead();
        }

        // The remote is empty and so it must be missing all entries.
        if remote.is_empty() {
            return Diff::missing_all();
        }

        // The local and remote both contain one "height" in their frontier meaning neither are
        // currently in a forked state.
        if local.len() == 1 && remote.len() == 1 {
            // We can safely unwrap the first (and only) item in the vectors.
            let (local_hash, local_seq_num) = local.inner().iter().next().unwrap();
            let (remote_hash, remote_seq_num) = remote.inner().iter().next().unwrap();

            // If the sequence numbers are equal, but the hashes are _not_ then the logs have
            // diverged into different branches and should be considered in an "ambiguous" state.
            if local_seq_num == remote_seq_num && local_hash != remote_hash {
                return Diff::ambiguous(local, remote);
            }

            // Compare sequence numbers to determine if the remote is ahead or behind.
            //
            // NOTE: even if the remote is determined to be missing entries because their sequence
            // number is lower than that of the local, it's not possible to infer that they are in
            // the same branch as the local replica. To be sure the replicas are not in a
            // diverging "forked" state (and that a linear sequence of entries can be sent) any
            // sync protocol implementation must check that the entry identified by the "from"
            // hash is present on the local replica, and if it is _not_ then all local entries
            // must be sent in order to assure the two replicas eventually converge to the same
            // state.
            if remote_seq_num < local_seq_num {
                return Diff::missing_from(remote_hash, remote_seq_num);
            } else {
                return Diff::ahead();
            }
        }

        // If the remote is a subset of the local then the remote is ahead.
        if remote.inner().is_subset(local.inner()) {
            return Diff::Ahead;
        }

        // If the local is a superset of the remote then the local is ahead.
        if local.inner().is_superset(remote.inner()) {
            return Diff::forked_behind(local, remote);
        }

        // All other cases can be understood as "ambiguous", meaning that the states are diverging
        // and neither is a subset of the other.
        Diff::ambiguous(local, remote)
    }

    pub fn inner(&self) -> &HashSet<(Hash, SeqNum)> {
        &self.0
    }

    pub fn inner_mut(&mut self) -> &mut HashSet<(Hash, SeqNum)> {
        &mut self.0
    }
}

impl<const N: usize> From<[(Hash, SeqNum); N]> for StateVector {
    fn from(arr: [(Hash, SeqNum); N]) -> Self {
        Self(HashSet::from_iter(arr))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Height<ID = Hash> {
    #[default]
    None,
    Entry {
        hash: ID,
        seq_num: SeqNum,
    },
}

impl Height<Hash> {
    pub fn new(hash: Hash, seq_num: SeqNum) -> Self {
        Self::Entry { hash, seq_num }
    }

    pub fn start() -> Self {
        Self::None
    }

    pub fn hash(&self) -> Option<Hash> {
        match self {
            Height::None => None,
            Height::Entry { hash, .. } => Some(*hash),
        }
    }

    pub fn seq_num(&self) -> Option<SeqNum> {
        match self {
            Height::None => None,
            Height::Entry { seq_num, .. } => Some(*seq_num),
        }
    }
}

impl From<Height<Hash>> for StateVector {
    fn from(val: Height<Hash>) -> Self {
        match val {
            Height::None => StateVector::new(),
            Height::Entry { hash, seq_num } => StateVector::from([(hash, seq_num)]),
        }
    }
}

/// The result of comparing "local" and "remote" state vectors of possibly forked logs.
#[derive(Debug, Default, PartialEq)]
pub enum Diff {
    /// The remote log is in an equal state.
    Equal,

    /// The remote log is in a further progressed state.
    Ahead,

    /// The remote log contains no entries.
    #[default]
    MissingAll,

    /// The remote log is behind the local. The hash and seq number of the remote logs' current
    /// height are returned.
    ///
    /// A "fork-aware" sync protocol must assert that an entry for this particular hash exists in
    /// the local log replica in order to assure that the remote replica is not in a divergent
    /// state. If the hash is missing then _all_ entries can be sent in order for both replicas to
    /// converge to a consistent (but still forked) state.
    Behind { hash: Hash, seq_num: SeqNum },

    /// The remote state vector is a subset of the local, this can be interpreted as the remote
    /// being "behind" the local and either the _difference_ in entries should be sent or (less
    /// efficiently) pessimistically send _all_ entries.
    ForkedBehind {
        local: StateVector,
        remote: StateVector,
    },

    /// The local and/or remote is in a forked state and vectors are not equal or a subset of
    /// one-another.
    ///
    /// In order to handle this case a sync protocol implementation must either calculate the
    /// _exact_ entries missing from the remote using a graph traversal algorithm, or
    /// alternatively send _all_ entries held locally for a log so that eventually equal state
    /// will be reached.
    Ambiguous {
        local: StateVector,
        remote: StateVector,
    },
}

impl Diff {
    fn equal() -> Self {
        Diff::Equal
    }

    fn ahead() -> Self {
        Diff::Ahead
    }

    fn missing_all() -> Self {
        Diff::MissingAll
    }

    fn missing_from(hash: &Hash, seq_num: &SeqNum) -> Self {
        Diff::Behind {
            hash: *hash,
            seq_num: *seq_num,
        }
    }

    fn ambiguous(local: &StateVector, remote: &StateVector) -> Self {
        Diff::Ambiguous {
            local: local.to_owned(),
            remote: remote.to_owned(),
        }
    }

    fn forked_behind(local: &StateVector, remote: &StateVector) -> Self {
        Diff::ForkedBehind {
            local: local.to_owned(),
            remote: remote.to_owned(),
        }
    }
}

/// Compare a local and remote log replica, calculate the difference and return a map containing
/// what should be sent to the remote in order to assure the replicas eventually converge towards
/// the same state.
///
/// This algorithm supports diffing logs that may be forked and pruned. It takes a "pessimistic"
/// approach where if an ambiguous state diff is detected (ie. both local and remote replicas are
/// forked) then _all_ local entries are sent to the remote. In this way replicas will converge to
/// a consistent state. An optimisation here would be to use a graph based diffing algorithm to
/// determine exactly the entries missing from a remote based on their state vector.
pub fn calculate_diff<L>(
    local: &HashMap<PublicKey, HashMap<L, StateVector>>,
    remote: &HashMap<PublicKey, HashMap<L, StateVector>>,
) -> HashMap<PublicKey, HashMap<L, Height>>
where
    L: Clone + StdHash + Eq + PartialEq,
{
    let mut remote_needs_from: HashMap<PublicKey, HashMap<L, Height>> = HashMap::default();

    for (public_key, local_logs) in local {
        for (log_id, local_frontier) in local_logs {
            // In all cases when the author or log was not present in the remotes' state vector
            // map then the default "missing all" diff is returned.
            let diff = remote
                .get(public_key)
                .and_then(|logs| logs.get(log_id))
                .map(|remote_frontier| local_frontier.diff(remote_frontier))
                .unwrap_or_default();

            let from = match diff {
                // If the remote has equal state, or is ahead of local, they don't need to be sent
                // any entries.
                Diff::Equal | Diff::Ahead => continue,

                // If the remote is behind the local, then it should be sent all entries after
                // it's current log height.
                Diff::Behind { hash, seq_num } => Height::new(hash, seq_num),

                // If the difference in state between the two logs is "ambiguous" or "forked
                // behind" then the local should pessimistically send all log entries to the
                // remote.
                Diff::Ambiguous { .. } | Diff::ForkedBehind { .. } => Height::start(),

                // The remote has no entries from this log so we the local should send everything
                // they have.
                Diff::MissingAll => Height::start(),
            };

            remote_needs_from
                .entry(*public_key)
                .or_default()
                .insert(log_id.to_owned(), from);
        }
    }

    remote_needs_from
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::logs::{Height, StateVector, calculate_diff};
    use crate::{Hash, PrivateKey};

    #[test]
    fn remote_behind() {
        let alice = PrivateKey::new().public_key();

        let local_height = Height::new(Hash::new(b"a"), 4);
        let local_logs = [(0, local_height.into())].into_iter().collect();
        let local = HashMap::from([(alice, local_logs)]);

        let remote_height = Height::new(Hash::new(b"b"), 2);
        let remote_logs = [(0, remote_height.clone().into())].into_iter().collect();
        let remote = HashMap::from([(alice, remote_logs)]);

        let result = calculate_diff(&local, &remote);
        let alice_logs = result.get(&alice).unwrap();
        assert_eq!(alice_logs, &HashMap::from([(0, remote_height)]))
    }

    #[test]
    fn ambiguous_forking() {
        let alice = PrivateKey::new().public_key();

        let local_height = Height::new(Hash::new(b"a"), 4);
        let local_logs = [(0, local_height.into())].into_iter().collect();
        let local = HashMap::from([(alice, local_logs)]);

        let remote_height = Height::new(Hash::new(b"b"), 4);
        let remote_logs = [(0, remote_height.clone().into())].into_iter().collect();

        let remote = HashMap::from([(alice, remote_logs)]);
        let result = calculate_diff(&local, &remote);
        let alice_logs = result.get(&alice).unwrap();
        assert_eq!(alice_logs, &HashMap::from([(0, Height::default())]))
    }

    #[test]
    fn remote_equal() {
        let alice = PrivateKey::new().public_key();

        let local_height = Height::new(Hash::new(b"x"), 5);
        let local_logs = [(0, local_height.clone().into())].into_iter().collect();
        let local = HashMap::from([(alice, local_logs)]);

        let remote_height = local_height.clone();
        let remote_logs = [(0, remote_height.into())].into_iter().collect();
        let remote = HashMap::from([(alice, remote_logs)]);

        let result = calculate_diff(&local, &remote);
        assert!(result.get(&alice).is_none())
    }

    #[test]
    fn remote_ahead() {
        let alice = PrivateKey::new().public_key();

        let local_height = Height::new(Hash::new(b"x"), 2);
        let local_logs = [(0, local_height.into())].into_iter().collect();
        let local = HashMap::from([(alice, local_logs)]);

        let remote_height = Height::new(Hash::new(b"y"), 5);
        let remote_logs = [(0, remote_height.into())].into_iter().collect();
        let remote = HashMap::from([(alice, remote_logs)]);

        let result = calculate_diff(&local, &remote);
        assert!(result.get(&alice).is_none())
    }

    #[test]
    fn remote_missing_log() {
        let alice = PrivateKey::new().public_key();

        let local_height = Height::new(Hash::new(b"a"), 3);
        let local_logs = [(0, local_height.into())].into_iter().collect();
        let local = HashMap::from([(alice, local_logs)]);

        let remote_logs = HashMap::new();
        let remote = HashMap::from([(alice, remote_logs)]);

        let result = calculate_diff(&local, &remote);
        let alice_logs = result.get(&alice).unwrap();
        assert_eq!(alice_logs, &HashMap::from([(0, Height::start())]));
    }

    #[test]
    fn remote_missing_author() {
        let alice = PrivateKey::new().public_key();

        let local_height = Height::new(Hash::new(b"a"), 7);
        let local_logs = [(0, local_height.into())].into_iter().collect();
        let local = HashMap::from([(alice, local_logs)]);

        let remote = HashMap::new();

        let result = calculate_diff(&local, &remote);
        let alice_logs = result.get(&alice).unwrap();
        assert_eq!(alice_logs, &HashMap::from([(0, Height::start())]));
    }

    #[test]
    fn local_and_remote_forked() {
        let alice = PrivateKey::new().public_key();

        let local_frontier = StateVector::from([(Hash::new(b"a"), 4), (Hash::new(b"b"), 4)]);
        let local_logs = [(0, local_frontier.clone())].into_iter().collect();
        let local = HashMap::from([(alice, local_logs)]);

        let remote_frontier = StateVector::from([(Hash::new(b"c"), 4), (Hash::new(b"d"), 4)]);
        let remote_logs = [(0, remote_frontier.clone())].into_iter().collect();
        let remote = HashMap::from([(alice, remote_logs)]);

        let result = calculate_diff(&local, &remote);
        let alice_logs = result.get(&alice).unwrap();
        assert_eq!(alice_logs, &HashMap::from([(0, Height::start())]))
    }

    #[test]
    fn local_subset_of_remote() {
        let alice = PrivateKey::new().public_key();

        let local_frontier = StateVector::from([(Hash::new(b"a"), 3), (Hash::new(b"b"), 3)]);
        let local_logs = [(0, local_frontier.clone())].into_iter().collect();
        let local = HashMap::from([(alice, local_logs)]);

        let mut remote_frontier = local_frontier.clone();
        remote_frontier.inner_mut().insert((Hash::new(b"c"), 3));
        let remote_logs = [(0, remote_frontier.clone())].into_iter().collect();
        let remote = HashMap::from([(alice, remote_logs)]);

        let result = calculate_diff(&local, &remote);
        assert!(result.get(&alice).is_none())
    }
}
