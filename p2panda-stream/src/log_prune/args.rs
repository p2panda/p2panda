// SPDX-License-Identifier: MIT OR Apache-2.0

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum LogPruneArgs<A, L, S> {
    PruneEntriesUntil {
        author: A,
        log_id: L,
        seq_num: S,
    },
    #[default]
    Ignore,
}
