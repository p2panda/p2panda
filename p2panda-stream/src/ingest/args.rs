// SPDX-License-Identifier: MIT OR Apache-2.0

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IngestArgs<L, TP> {
    pub log_id: L,
    pub topic: TP,
    pub prune_flag: bool,
}
