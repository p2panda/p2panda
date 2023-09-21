// SPDX-License-Identifier: AGPL-3.0-or-later

//! Helper methods for working with p2panda data types.
use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;

use crate::api::validation::get_expected_skiplink;
use crate::api::DomainError;
use crate::entry::traits::AsEncodedEntry;
use crate::entry::{LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::PublicKey;
use crate::storage_provider::traits::EntryStore;

/// Retrieve the expected skiplink for the entry identified by public key, log id and sequence
/// number.
pub async fn get_skiplink_for_entry<S: EntryStore>(
    store: &S,
    seq_num: &SeqNum,
    log_id: &LogId,
    public_key: &PublicKey,
) -> Result<Option<Hash>, DomainError> {
    // Check if skiplink is required and return hash if so
    let skiplink = if is_lipmaa_required(seq_num.as_u64()) {
        Some(get_expected_skiplink(store, public_key, log_id, seq_num).await?)
    } else {
        None
    }
    .map(|entry| entry.hash());

    Ok(skiplink)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::entry::traits::AsEncodedEntry;
    use crate::entry::{LogId, SeqNum};
    use crate::storage_provider::traits::EntryStore;
    use crate::test_utils::fixtures::populate_store_config;
    use crate::test_utils::memory_store::helpers::{populate_store, PopulateStoreConfig};
    use crate::test_utils::memory_store::MemoryStore;

    use super::get_skiplink_for_entry;

    #[rstest]
    #[tokio::test]
    async fn gets_skiplink_for_entry(
        #[from(populate_store_config)]
        #[with(4, 1, 1)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        let (key_pairs, _) = populate_store(&store, &config).await;

        let public_key = key_pairs[0].public_key();

        // Request skiplink hash for entry at seq num 1 which should be None
        let skiplink = get_skiplink_for_entry(
            &store,
            &SeqNum::new(1).unwrap(),
            &LogId::new(0),
            &public_key,
        )
        .await
        .unwrap();
        assert!(skiplink.is_none());

        // Request skiplink hash for entry at seq num 4 which should be Some(<hash_of_entry_seq_num_1>)
        let skiplink = get_skiplink_for_entry(
            &store,
            &SeqNum::new(4).unwrap(),
            &LogId::new(0),
            &public_key,
        )
        .await
        .unwrap();

        // Get the entry at seq number 1 (which is the expected skiplink)
        let entry_one_hash = store
            .get_entry_at_seq_num(&public_key, &LogId::new(0), &SeqNum::new(1).unwrap())
            .await
            .unwrap()
            .map(|entry| entry.hash());

        assert!(skiplink.is_some());
        assert_eq!(skiplink, entry_one_hash);

        // Request skiplink hash for entry at seq num 40 which should error (as this sequence
        // number requires a skiplink but it doesn't exist in the store)
        let skiplink = get_skiplink_for_entry(
            &store,
            &SeqNum::new(40).unwrap(),
            &LogId::new(0),
            &public_key,
        )
        .await;

        assert!(skiplink.is_err());
    }
}
