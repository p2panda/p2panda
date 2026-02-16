// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use p2panda_core::{Extensions, Hash, Operation};

use crate::logs::traits::SeqNum;
use crate::logs::{LogId, LogStore};
use crate::sqlite::{SqliteError, SqliteStore};

impl<'a, L, E> LogStore<Operation<E>, L, Hash> for SqliteStore<'a>
where
    E: Extensions,
    L: LogId,
{
    type Error = SqliteError;

    async fn get_log_height(
        &self,
        public_key: &p2panda_core::PublicKey,
        log_id: &L,
    ) -> Result<Option<(Hash, SeqNum)>, Self::Error> {
        todo!()
    }

    async fn get_log_heights(
        &self,
        author: &p2panda_core::PublicKey,
        logs: &[L],
    ) -> Result<Option<std::collections::HashMap<L, SeqNum>>, Self::Error> {
        todo!()
    }

    async fn get_log_size(
        &self,
        public_key: &p2panda_core::PublicKey,
        log_id: &L,
        after: Option<SeqNum>,
        until: Option<SeqNum>,
    ) -> Result<Option<(u64, u64)>, Self::Error> {
        todo!()
    }

    async fn get_log_entries(
        &self,
        public_key: &p2panda_core::PublicKey,
        log_id: &L,
        after: Option<SeqNum>,
        until: Option<SeqNum>,
        // @TODO: we actually want a wrapper type here so that we can easily access the serialized
        // header and body bytes. 
    ) -> Result<Option<Vec<Operation<E>>>, Self::Error> {
        todo!()
    }

    async fn prune_entries(
        &self,
        author: &p2panda_core::PublicKey,
        log_id: &L,
        until: &SeqNum,
    ) -> Result<bool, Self::Error> {
        todo!()
    }

    async fn get_log_entries_batch(
        &self,
        public_key: &p2panda_core::PublicKey,
        ranges: &HashMap<L, (Option<SeqNum>, Option<SeqNum>)>,
    ) -> Result<Vec<Operation<E>>, Self::Error> {
        todo!()
    }

    async fn get_log_size_batch(
        &self,
        public_key: &p2panda_core::PublicKey,
        ranges: &HashMap<L, (Option<SeqNum>, Option<SeqNum>)>,
    ) -> Result<(u64, u64), Self::Error> {
        todo!()
    }
}
