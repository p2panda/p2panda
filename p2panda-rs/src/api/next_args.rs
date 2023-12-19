// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::api::validation::get_checked_document_id_for_view_id;
use crate::api::DomainError;
use crate::document::DocumentViewId;
use crate::identity::PublicKey;
use crate::storage_provider::traits::OperationStore;

type Depth = ();

pub async fn next_args<S: OperationStore>(
    store: &S,
    _public_key: &PublicKey,
    document_view_id: &DocumentViewId,
) -> Result<(Depth), DomainError> {
    // Get the document_id for this document_view_id. This performs several validation steps (check
    // method doc string).
    let document_id = get_checked_document_id_for_view_id(store, document_view_id).await?;

    // @TODO: Get the current document depth (global or just for this author).

    Ok(()) // actually we return the depth here.
}

// #[cfg(test)]
// mod tests {
//     use rstest::rstest;
//
//     use crate::api::next_args;
//     use crate::api::next_args::{calculate_next_args_existing_log, calculate_next_args_new_log};
//     use crate::document::DocumentViewId;
//     use crate::entry::encode::sign_and_encode_entry;
//     use crate::entry::traits::{AsEncodedEntry, AsEntry};
//     use crate::entry::{LogId, SeqNum};
//     use crate::identity::KeyPair;
//     use crate::operation::OperationId;
//     use crate::storage_provider::traits::EntryStore;
//     use crate::test_utils::constants::PRIVATE_KEY;
//     use crate::test_utils::fixtures::{
//         key_pair, populate_store_config, random_document_view_id, random_hash,
//     };
//     use crate::test_utils::memory_store::helpers::{
//         populate_store, remove_entries, remove_operations, PopulateStoreConfig,
//     };
//     use crate::test_utils::memory_store::{MemoryStore, StorageEntry};
//
//     type LogIdAndSeqNum = (u64, u64);
//
//     #[rstest]
//     #[tokio::test]
//     async fn calculates_next_args(
//         #[from(populate_store_config)]
//         #[with(8, 1, 1)]
//         config: PopulateStoreConfig,
//     ) {
//         let store = MemoryStore::default();
//         let (key_pairs, _) = populate_store(&store, &config).await;
//
//         let public_key = key_pairs[0].public_key();
//
//         // Calculate next args for a new log of the existing public key.
//         let (backlink, skiplink, seq_num, log_id) =
//             calculate_next_args_new_log(&store, &public_key)
//                 .await
//                 .unwrap();
//
//         assert_eq!(backlink, None);
//         assert_eq!(skiplink, None);
//         assert_eq!(seq_num, SeqNum::default());
//         assert_eq!(log_id, LogId::new(1));
//
//         // Calculate next args for an existing log and public key.
//         let (backlink, skiplink, seq_num, log_id) =
//             calculate_next_args_existing_log(&store, &LogId::new(0), &public_key)
//                 .await
//                 .unwrap();
//
//         // Get expected backlink from the store.
//         let expected_backlink = store
//             .get_entry_at_seq_num(&public_key, &LogId::new(0), &SeqNum::new(8).unwrap())
//             .await
//             .unwrap()
//             .unwrap();
//
//         assert_eq!(backlink, Some(expected_backlink.hash()));
//         assert_eq!(skiplink, None);
//         assert_eq!(seq_num, SeqNum::new(9).unwrap());
//         assert_eq!(log_id, LogId::new(0));
//
//         // Should error because this method doesn't handle next args for a new log.
//         let result = calculate_next_args_existing_log(&store, &LogId::new(1), &public_key).await;
//         assert!(result.is_err())
//     }
//
//     #[rstest]
//     #[case::ok_single_writer(
//         &[],
//         &[(0, 8)],
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
//     )]
//     #[case::ok_many_previous(
//         &[],
//         &[(0, 8), (0, 7), (0, 6)],
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
//     )]
//     #[case::ok_not_the_most_recent_document_view_id(
//         &[],
//         &[(0, 1)],
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
//     )]
//     #[case::ok_multi_writer(
//         &[],
//         &[(0, 8)],
//         KeyPair::new()
//     )]
//     #[should_panic(
//         expected = "Previous operation 00202df2f7c15280a319f42f1b2df51cd8dcaa79286428ff48301309d3bb37868981 not found in store"
//     )]
//     #[case::previous_operation_missing(
//         &[(0, 8)],
//         &[(0, 8)],
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
//     )]
//     #[should_panic(
//         expected = "Previous operation 0020397d5f246d6124d1aa6fb5fcdb2a0f202bafe0aecb6ff1423fa2164ae4403204 not found in store"
//     )]
//     #[case::one_of_some_previous_missing(
//         &[(0, 7)],
//         &[(0, 7), (0, 8)],
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
//     )]
//     #[should_panic(
//         expected = "Previous operation 00202df2f7c15280a319f42f1b2df51cd8dcaa79286428ff48301309d3bb37868981 not found in store"
//     )]
//     #[case::one_of_some_previous_missing(
//         &[(0, 8)],
//         &[(0, 7), (0, 8)],
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
//     )]
//     #[should_panic(
//         expected = "Previous operation 00202df2f7c15280a319f42f1b2df51cd8dcaa79286428ff48301309d3bb37868981 not found in store"
//     )]
//     #[case::missing_previous_operation_multi_writer(
//         &[(0, 8)],
//         &[(0, 8)],
//         KeyPair::new()
//     )]
//     #[should_panic(
//         expected = "Operations in passed document view id originate from different documents"
//     )]
//     #[case::previous_invalid_multiple_document_id(
//         &[],
//         &[(0, 8), (1, 8)],
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
//     )]
//     #[tokio::test]
//     async fn next_args_with_missing_operations(
//         #[case] operations_to_remove: &[LogIdAndSeqNum],
//         #[case] document_view_id: &[LogIdAndSeqNum],
//         #[case] key_pair: KeyPair,
//         #[from(populate_store_config)]
//         #[with(8, 2, 1)]
//         config: PopulateStoreConfig,
//     ) {
//         let store = MemoryStore::default();
//         let (key_pairs, _) = populate_store(&store, &config).await;
//
//         let public_key_with_removed_operations = key_pairs[0].public_key();
//         let public_key_making_request = key_pair.public_key();
//
//         // Map the passed &[LogIdAndSeqNum] into a DocumentViewId containing the claimed operations.
//         let document_view_id: Vec<OperationId> = document_view_id
//             .iter()
//             .filter_map(|(log_id, seq_num)| {
//                 store
//                     .entries
//                     .lock()
//                     .unwrap()
//                     .values()
//                     .find(|entry| {
//                         entry.seq_num().as_u64() == *seq_num
//                             && entry.log_id().as_u64() == *log_id
//                             && *entry.public_key() == public_key_with_removed_operations
//                     })
//                     .map(|entry| entry.hash().into())
//             })
//             .collect();
//
//         // Construct document view id for previous operations.
//         let document_view_id = DocumentViewId::new(&document_view_id);
//
//         // Remove some operations.
//         remove_operations(
//             &store,
//             &public_key_with_removed_operations,
//             operations_to_remove,
//         );
//
//         // Get the next args.
//         let result = next_args(&store, &public_key_making_request, Some(&document_view_id)).await;
//
//         // Unwrap here causing a panic, we check the errors match what we expect.
//         result.map_err(|err| err.to_string()).unwrap();
//     }
//
//     type SeqNumU64 = u64;
//     type LogIdU64 = u64;
//     type Backlink = Option<u64>;
//     type Skiplink = Option<u64>;
//
//     #[rstest]
//     #[case(0, 0, None, (1, 0, None, None))]
//     #[case(1, 1, Some((1, 0)), (2, 0, Some(1), None))]
//     #[case(2, 1, Some((2, 0)), (3, 0, Some(2), None))]
//     #[case(3, 1, Some((3, 0)), (4, 0, Some(3), Some(1)))]
//     #[case(4, 1, Some((4, 0)), (5, 0, Some(4), None))]
//     #[case(5, 1, Some((5, 0)), (6, 0, Some(5), None))]
//     #[case(6, 1, Some((6, 0)), (7, 0, Some(6), None))]
//     #[case(7, 1, Some((7, 0)), (8, 0, Some(7), Some(4)))]
//     #[case(2, 1, Some((1, 0)), (3, 0, Some(2), None))]
//     #[case(3, 1, Some((1, 0)), (4, 0, Some(3), Some(1)))]
//     #[case(4, 1, Some((1, 0)), (5, 0, Some(4), None))]
//     #[case(5, 1, Some((1, 0)), (6, 0, Some(5), None))]
//     #[case(6, 1, Some((1, 0)), (7, 0, Some(6), None))]
//     #[case(7, 1, Some((1, 0)), (8, 0, Some(7), Some(4)))]
//     #[case(1, 2, None, (1, 2, None, None))]
//     #[case(1, 10, None, (1, 10, None, None))]
//     #[case(1, 100, None, (1, 100, None, None))]
//     #[case(1, 100, Some((1, 9)), (2, 9, Some(1), None))]
//     #[case(1, 100, Some((1, 99)), (2, 99, Some(1), None))]
//     #[tokio::test]
//     async fn next_args_with_expected_results(
//         #[case] no_of_entries: usize,
//         #[case] no_of_logs: usize,
//         #[case] document_view_id: Option<(SeqNumU64, LogIdU64)>,
//         #[case] expected_next_args: (SeqNumU64, LogIdU64, Backlink, Skiplink),
//     ) {
//         let store = MemoryStore::default();
//         // Populate the db with the number of entries defined in the test params.
//         let config = PopulateStoreConfig {
//             no_of_entries,
//             no_of_logs,
//             no_of_public_keys: 1,
//             ..PopulateStoreConfig::default()
//         };
//         let (key_pairs, _) = populate_store(&store, &config).await;
//
//         // The public key of the author who published the entries.
//         let public_key = key_pairs[0].public_key();
//
//         // Construct the passed document view id (specified by sequence number and log id)
//         let document_view_id: Option<DocumentViewId> = document_view_id.map(|(seq_num, log_id)| {
//             store
//                 .entries
//                 .lock()
//                 .unwrap()
//                 .values()
//                 .find(|entry| {
//                     entry.seq_num().as_u64() == seq_num && entry.log_id().as_u64() == log_id
//                 })
//                 .map(|entry| DocumentViewId::new(&[entry.hash().into()]))
//                 .unwrap()
//         });
//
//         // Construct the expected next args
//         let expected_seq_num = SeqNum::new(expected_next_args.0).unwrap();
//         let expected_log_id = LogId::new(expected_next_args.1);
//         let expected_backlink = match expected_next_args.2 {
//             Some(backlink) => store
//                 .get_entry_at_seq_num(
//                     &public_key,
//                     &expected_log_id,
//                     &SeqNum::new(backlink).unwrap(),
//                 )
//                 .await
//                 .unwrap()
//                 .map(|entry| entry.hash()),
//             None => None,
//         };
//         let expected_skiplink = match expected_next_args.3 {
//             Some(skiplink) => store
//                 .get_entry_at_seq_num(
//                     &public_key,
//                     &expected_log_id,
//                     &SeqNum::new(skiplink).unwrap(),
//                 )
//                 .await
//                 .unwrap()
//                 .map(|entry| entry.hash()),
//             None => None,
//         };
//
//         // Request next args for the public key and document view.
//         let (backlink, skiplink, seq_num, log_id) =
//             next_args(&store, &public_key, document_view_id.as_ref())
//                 .await
//                 .unwrap();
//
//         assert_eq!(backlink, expected_backlink);
//         assert_eq!(skiplink, expected_skiplink);
//         assert_eq!(seq_num, expected_seq_num);
//         assert_eq!(log_id, expected_log_id);
//     }
//
//     #[rstest]
//     #[tokio::test]
//     async fn gets_next_args_other_cases(
//         #[from(populate_store_config)]
//         #[with(7, 1, 1)]
//         config: PopulateStoreConfig,
//     ) {
//         let store = MemoryStore::default();
//         let (key_pairs, documents) = populate_store(&store, &config).await;
//
//         // The public key of the author who published the entries.
//         let public_key = key_pairs[0].public_key();
//
//         // Get with no DocumentViewId given.
//         let (backlink, skiplink, seq_num, log_id) =
//             next_args(&store, &public_key, None).await.unwrap();
//
//         assert_eq!(backlink, None);
//         assert_eq!(skiplink, None);
//         assert_eq!(seq_num, SeqNum::new(1).unwrap());
//         assert_eq!(log_id, LogId::new(1));
//
//         // Get with non-existent DocumentViewId given.
//         let result = next_args(&store, &public_key, Some(&random_document_view_id())).await;
//         assert!(result.is_err());
//         assert!(
//             result
//                 .unwrap_err()
//                 .to_string()
//                 .contains("not found in store") // This is a partial string match, preceded by "Previous operation <XXXXXX...>"
//         );
//
//         // Here we are missing the skiplink.
//         remove_entries(&store, &public_key, &[(0, 4)]);
//         let document_id = documents.get(0).unwrap();
//         let document_view_id = DocumentViewId::new(&[document_id.as_str().parse().unwrap()]);
//
//         let result = next_args(&store, &public_key, Some(&document_view_id)).await;
//         assert_eq!(
//             result.unwrap_err().to_string(),
//             "Expected skiplink entry not found in store: public key 2f8e50c2ede6d936ecc3144187ff1c273808185cfbc5ff3d3748d1ff7353fc96, log id 0, seq num 4"
//         );
//     }
//
//     #[rstest]
//     #[should_panic(expected = "Document is deleted")]
//     #[case(KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
//     #[should_panic(expected = "Document is deleted")]
//     #[case(KeyPair::new())]
//     #[tokio::test]
//     async fn next_args_deleted_documents(
//         #[case] key_pair: KeyPair,
//         #[from(populate_store_config)]
//         #[with(3, 1, 1, true)]
//         config: PopulateStoreConfig,
//     ) {
//         let store = MemoryStore::default();
//         let (_, documents) = populate_store(&store, &config).await;
//
//         let document_id = documents.first().unwrap();
//         let document_view_id: DocumentViewId = document_id.as_str().parse().unwrap();
//         let public_key = key_pair.public_key();
//
//         let result = next_args(&store, &public_key, Some(&document_view_id)).await;
//
//         result.map_err(|err| err.to_string()).unwrap();
//     }
//
//     #[rstest]
//     #[should_panic(
//         expected = "Max sequence number reached for public key 2f8e50c2ede6d936ecc3144187ff1c273808185cfbc5ff3d3748d1ff7353fc96 log 0"
//     )]
//     #[tokio::test]
//     async fn next_args_max_seq_num_reached(
//         key_pair: KeyPair,
//         #[from(populate_store_config)]
//         #[with(2, 1, 1, false)]
//         config: PopulateStoreConfig,
//     ) {
//         let store = MemoryStore::default();
//         let _ = populate_store(&store, &config).await;
//
//         let public_key = key_pair.public_key();
//
//         let entry_two = store
//             .get_entry_at_seq_num(&public_key, &LogId::default(), &SeqNum::new(2).unwrap())
//             .await
//             .unwrap()
//             .unwrap();
//
//         let encoded_entry = sign_and_encode_entry(
//             &LogId::default(),
//             &SeqNum::new(u64::MAX).unwrap(),
//             Some(&random_hash()),
//             Some(&random_hash()),
//             entry_two.payload.as_ref().unwrap(),
//             &key_pair,
//         )
//         .unwrap();
//
//         let entry = StorageEntry::new(&encoded_entry, entry_two.payload.as_ref());
//
//         store
//             .entries
//             .lock()
//             .unwrap()
//             .insert(entry.hash(), entry.clone());
//
//         let result = next_args(&store, &public_key, Some(&entry_two.hash().into())).await;
//
//         result.map_err(|err| err.to_string()).unwrap();
//     }
// }
