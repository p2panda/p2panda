// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

use crate::identity::Author;

/// Uniquely identify a single-author log.
///
/// The `LogId` exists purely to group a set of operations and is intended to be implemented for
/// any type which meets the design requirements of a particular application.
///
/// A blanket implementation is provided for any type meeting the required trait bounds.
///
/// Here we briefly outline several implementation scenarios:
///
/// An application relying on a one-log-per-author design might choose to implement `LogId` for a
/// thin wrapper around an Ed25519 public key; this effectively ties the log to the public key of
/// the author. Secure Scuttlebutt (SSB) is an example of a protocol which relies on this model.
///
/// In an application where one author may produce operations grouped into multiple logs, `LogId`
/// might be represented a unique number for each log instance.
///
/// Some applications might require semantic grouping of operations. For example, a chat
/// application may choose to create a separate log for each author-channel pairing. In such a
/// scenario, `LogId` might be implemented for a `struct` containing a `String` representation of
/// the channel name.
///
/// Finally, please note that implementers of `LogId` must take steps to ensure their log design
/// is fit for purpose and that all operations have been thoroughly validated before being
/// persisted. No such validation checks are provided by `p2panda-store`.
pub trait LogId: Clone + Eq + Ord + StdHash + Serialize + for<'de> Deserialize<'de> {}

impl<T> LogId for T where T: Clone + Eq + Ord + StdHash + Serialize + for<'de> Deserialize<'de> {}

/// Sequence number of an entry in an append-only log.
pub type SeqNum = u64;

/// Map of log heights grouped by author.
pub type LogHeights<A, L> = BTreeMap<A, BTreeMap<L, SeqNum>>;

/// Map of log ranges grouped by author.
pub type LogRanges<A, L> = BTreeMap<A, BTreeMap<L, (Option<SeqNum>, Option<SeqNum>)>>;

/// Compare two sets of logs (local and remote) and calculate the "diff" representing ranges of
/// entries that should be sent to the remote.
///
/// Local and remote states are represented by a map of authors to logs, where the logs are
/// represented by their unique identifier and current log height. If the remote is not aware of a
/// log, then the range containing all entries in the local log will be contained in the diff, if
/// the remote knows of some entries in a log, then the range representing only the entries they
/// need will be included.
///
/// Log ranges are represented by `(Option<u64>, Option<u64>)` tuples where the first value is an
/// exclusive "from" sequence number and the later is an inclusive "until" sequence number. If
/// either values are `None` that signifies that all entries from the start, or to the end, are
/// required.
///
/// The returned ranges can be used in a sync protocol to then fetch entries from a store and send
/// them to the remote. If both local and remote replicas do this then they will arrive at the
/// same state. If pruned logs are being replicated and a range has been returned from this
/// method, then it is expected only the remaining "frontier" will be replicated for each log.
pub fn compare<A, L>(local: &LogHeights<A, L>, remote: &LogHeights<A, L>) -> LogRanges<A, L>
where
    A: Author,
    L: LogId,
{
    let mut remote_needs: LogRanges<A, L> = BTreeMap::default();

    // Iterate over all authors.
    for (public_key, local_logs) in local {
        let Some(remote_logs) = remote.get(public_key) else {
            // If the remote did not know of this author, then they need all entries in all of
            // their logs that the local knows of.
            let needs = local_logs
                .iter()
                .map(|(log_id, log_height)| (log_id.clone(), (None, Some(*log_height))))
                .collect();
            remote_needs.insert(public_key.to_owned(), needs);
            continue;
        };

        // If the local and remote logs are equal then nothing needs to be sent.
        if local_logs == remote_logs {
            continue;
        }

        // Iterate over all local logs for this author.
        for (log_id, local_log_height) in local_logs {
            let Some(remote_log_height) = remote_logs.get(log_id) else {
                // If the remote did not know of this log, then they need all entries from the
                // local.
                remote_needs
                    .entry(public_key.to_owned())
                    .or_default()
                    .insert(log_id.clone(), (None, Some(*local_log_height)));
                continue;
            };

            // If the remote log height is less than the local, then include the exact range they
            // need in the diff.
            if remote_log_height < local_log_height {
                remote_needs
                    .entry(public_key.to_owned())
                    .or_default()
                    .insert(
                        log_id.clone(),
                        (Some(*remote_log_height), Some(*local_log_height)),
                    );
            }
        }
    }

    remote_needs
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::logs::compare;

    type Author = u8;

    impl crate::identity::Author for Author {}

    const ALICE: Author = 0;
    const BOB: Author = 1;

    #[test]
    fn both_empty() {
        let local: BTreeMap<Author, BTreeMap<u64, u64>> = BTreeMap::new();
        let remote: BTreeMap<Author, BTreeMap<u64, u64>> = BTreeMap::new();
        let result = compare(&local, &remote);
        assert!(result.is_empty());
    }

    #[test]
    fn remote_empty() {
        let mut local: BTreeMap<Author, BTreeMap<u64, u64>> = BTreeMap::new();
        let logs = BTreeMap::from([(1, 5), (2, 10)]);
        local.insert(ALICE, logs);

        let remote: BTreeMap<Author, BTreeMap<u64, u64>> = BTreeMap::new();

        let result = compare(&local, &remote);
        let needs = result.get(&ALICE).unwrap();

        assert_eq!(needs.get(&1), Some(&(None, Some(5))));
        assert_eq!(needs.get(&2), Some(&(None, Some(10))));
    }

    #[test]
    fn remote_missing_single_log() {
        let mut local = BTreeMap::new();
        local.insert(ALICE, BTreeMap::from([(1, 5), (2, 10)]));

        let mut remote = BTreeMap::new();
        remote.insert(ALICE, BTreeMap::from([(1, 5)]));

        let result = compare(&local, &remote);
        let needs = result.get(&ALICE).unwrap();

        assert_eq!(needs.get(&2), Some(&(None, Some(10))));
        assert!(!needs.contains_key(&1));
    }

    #[test]
    fn remote_behind() {
        let mut local = BTreeMap::new();
        local.insert(ALICE, BTreeMap::from([(1, 20)]));

        let mut remote = BTreeMap::new();
        remote.insert(ALICE, BTreeMap::from([(1, 10)]));

        let result = compare(&local, &remote);
        let needs = result.get(&ALICE).unwrap();

        assert_eq!(needs.get(&1), Some(&(Some(10), Some(20))));
    }

    #[test]
    fn remote_ahead() {
        let mut local = BTreeMap::new();
        local.insert(ALICE, BTreeMap::from([(1, 20)]));

        let mut remote = BTreeMap::new();
        remote.insert(ALICE, BTreeMap::from([(1, 30)]));

        let result = compare(&local, &remote);
        assert!(result.is_empty());
    }

    #[test]
    fn equal() {
        let mut local = BTreeMap::new();
        local.insert(ALICE, BTreeMap::from([(1, 20)]));

        let mut remote = BTreeMap::new();
        remote.insert(ALICE, BTreeMap::from([(1, 20)]));

        let result = compare(&local, &remote);
        assert!(result.is_empty());
    }

    #[test]
    fn remote_missing_multiple_logs() {
        let mut local = BTreeMap::new();
        local.insert(ALICE, BTreeMap::from([(1, 5), (2, 10), (3, 15)]));

        let mut remote = BTreeMap::new();
        remote.insert(ALICE, BTreeMap::from([(1, 5)]));

        let result = compare(&local, &remote);
        let needs = result.get(&ALICE).unwrap();

        assert_eq!(needs.get(&2), Some(&(None, Some(10))));
        assert_eq!(needs.get(&3), Some(&(None, Some(15))));
        assert!(!needs.contains_key(&1));
    }

    #[test]
    fn remote_missing_author() {
        let mut local = BTreeMap::new();
        local.insert(ALICE, BTreeMap::from([(1, 5)]));
        local.insert(BOB, BTreeMap::from([(1, 5)]));

        let mut remote = BTreeMap::new();
        remote.insert(ALICE, BTreeMap::from([(1, 5)]));

        let result = compare(&local, &remote);
        let needs = result.get(&BOB).unwrap();

        assert_eq!(needs.get(&1), Some(&(None, Some(5))));
    }
}
