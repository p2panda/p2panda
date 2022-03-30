// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::DocumentId;
use crate::entry::SeqNum;
use crate::hash::Hash;
use crate::operation::AsOperation;
use crate::storage_provider::errors::PublishEntryError;
use crate::storage_provider::models::{EntryWithOperation, Log};
use crate::storage_provider::traits::{
    AsEntryArgsRequest, AsEntryArgsResponse, AsPublishEntryRequest, AsPublishEntryResponse,
    AsStorageEntry, AsStorageLog, EntryStore, LogStore,
};

/// Trait which handles all high level storage queries and insertions.
///
/// This trait should be implemented on the root storage provider struct. It's definitions make up
/// the the higher level methods a p2panda client needs for interacting with data storage.
#[async_trait]
pub trait StorageProvider<StorageEntry: AsStorageEntry, StorageLog: AsStorageLog>:
    EntryStore<StorageEntry> + LogStore<StorageLog>
{
    /// Params when making a request to get the next entry args for an author and document.
    type EntryArgsRequest: AsEntryArgsRequest + Sync;

    /// Response from a call to get next entry args for an author and document.
    type EntryArgsResponse: AsEntryArgsResponse;

    /// Params when making a request to publish a new entry.
    type PublishEntryRequest: AsPublishEntryRequest + Sync;

    /// Response from a call to publish a new entry.
    type PublishEntryResponse: AsPublishEntryResponse;

    /// Returns the related document for any entry.
    ///
    /// Every entry is part of a document and, through that, associated with a specific log id used
    /// by this document and author. This method returns that document id by looking up the log
    /// that the entry was stored in.
    async fn get_document_by_entry(
        &self,
        entry_hash: &Hash,
    ) -> Result<Option<DocumentId>, Box<dyn std::error::Error>>;

    /// Returns required data (backlink and skiplink entry hashes, last sequence number and the
    /// document's log_id) to encode a new bamboo entry.
    async fn get_entry_args(
        &self,
        params: &Self::EntryArgsRequest,
    ) -> Result<Self::EntryArgsResponse, Box<dyn std::error::Error>> {
        // Validate the entry args request parameters.
        params.validate()?;

        // Determine log_id for this document. If this is the very first operation in the document
        // graph, the `document` value is None and we will return the next free log id
        let log = self
            .find_document_log_id(params.author(), params.document_id().as_ref())
            .await?;

        // Determine backlink and skiplink hashes for the next entry. To do this we need the latest
        // entry in this log
        let entry_latest = self.latest_entry(params.author(), &log).await?;

        match entry_latest.clone() {
            // An entry was found which serves as the backlink for the upcoming entry
            Some(entry_backlink) => {
                let entry_latest = entry_latest.unwrap();
                let entry_hash_backlink = entry_backlink.hash();
                // Determine skiplink ("lipmaa"-link) entry in this log
                let entry_hash_skiplink = self.determine_skiplink(&entry_latest).await?;

                Ok(Self::EntryArgsResponse::new(
                    Some(entry_hash_backlink.clone()),
                    entry_hash_skiplink,
                    entry_latest.seq_num().clone().next().unwrap(),
                    entry_latest.log_id(),
                ))
            }
            // No entry was given yet, we can assume this is the beginning of the log
            None => Ok(Self::EntryArgsResponse::new(
                None,
                None,
                SeqNum::default(),
                log,
            )),
        }
    }

    /// Stores an author's Bamboo entry with operation payload in database after validating it.
    async fn publish_entry(
        &self,
        params: &Self::PublishEntryRequest,
    ) -> Result<Self::PublishEntryResponse, Box<dyn std::error::Error>> {
        // Create an `EntryWithOperation` which also validates the encoded entry and operation.
        let entry: StorageEntry =
            EntryWithOperation::new(params.entry_signed(), params.operation_encoded())?.into();

        // Every operation refers to a document we need to determine. A document is identified by the
        // hash of its first `CREATE` operation, it is the root operation of every document graph
        let document_id = if entry.operation().is_create() {
            // This is easy: We just use the entry hash directly to determine the document id
            DocumentId::new(entry.hash().into())
        } else {
            // For any other operations which followed after creation we need to either walk the operation
            // graph back to its `CREATE` operation or more easily look up the database since we keep track
            // of all log ids and documents there.
            //
            // We can determine the used document hash by looking at what we know about the previous
            // entry in this author's log.
            //
            // @TODO: This currently looks at the backlink, in the future we want to use
            // "previousOperation", since in a multi-writer setting there might be no backlink for
            // update operations! See: https://github.com/p2panda/aquadoggo/issues/49
            let backlink_entry_hash = entry
                .backlink_hash()
                .ok_or_else(|| PublishEntryError::OperationWithoutBacklink(entry.hash()))?;

            self.get_document_by_entry(&backlink_entry_hash)
                .await?
                // @TODO this trips if the backlink is missing not necessarily the document
                // needs revisiting when we use previous_operations to retrieve the correct
                // document.
                .ok_or_else(|| PublishEntryError::DocumentMissing(entry.hash()))?
        };

        // Determine expected log id for new entry
        let document_log_id = self
            .find_document_log_id(&entry.author(), Some(&document_id))
            .await?;

        // Check if provided log id matches expected log id
        if document_log_id != entry.log_id() {
            return Err(PublishEntryError::InvalidLogId(
                entry.log_id().as_u64(),
                document_log_id.as_u64(),
            )
            .into());
        }

        // Get related bamboo backlink and skiplink entries
        let entry_backlink_bytes = if !entry.seq_num().is_first() {
            self.entry_at_seq_num(
                &entry.author(),
                &entry.log_id(),
                &entry.seq_num().backlink_seq_num().unwrap(),
            )
            .await?
            .map(|link| {
                let bytes = link.entry_bytes();
                Some(bytes)
            })
            .ok_or_else(|| PublishEntryError::BacklinkMissing(entry.hash()))
        } else {
            Ok(None)
        }?;

        let entry_skiplink_bytes = if !entry.seq_num().is_first() {
            self.entry_at_seq_num(
                &entry.author(),
                &entry.log_id(),
                &entry.seq_num().skiplink_seq_num().unwrap(),
            )
            .await?
            .map(|link| {
                let bytes = link.entry_bytes();
                Some(bytes)
            })
            .ok_or_else(|| PublishEntryError::SkiplinkMissing(entry.hash()))
        } else {
            Ok(None)
        }?;

        // Verify bamboo entry integrity, including encoding, signature of the entry correct back-
        // and skiplinks
        bamboo_rs_core_ed25519_yasmf::verify(
            &entry.entry_bytes(),
            Some(&params.operation_encoded().to_bytes()),
            entry_skiplink_bytes.as_deref(),
            entry_backlink_bytes.as_deref(),
        )?;

        // Register log in database when a new document is created
        if entry.operation().is_create() {
            let log = Log::new(
                &entry.author(),
                &entry.operation().schema(),
                &document_id,
                &entry.log_id(),
            )
            .into();

            self.insert_log(log).await?;
        }

        // Finally insert Entry in database
        self.insert_entry(entry.clone()).await?;

        // Already return arguments for next entry creation
        let entry_latest: StorageEntry = self
            .latest_entry(&entry.author(), &entry.log_id())
            .await?
            .unwrap();
        let entry_hash_skiplink = self.determine_skiplink(&entry_latest).await?;
        let next_seq_num = entry_latest.seq_num().clone().next().unwrap();

        Ok(Self::PublishEntryResponse::new(
            Some(entry.hash()),
            entry_hash_skiplink,
            next_seq_num,
            entry.log_id(),
        ))
    }
}

#[cfg(test)]
pub mod tests {
    use std::convert::TryFrom;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use rstest::rstest;

    use crate::document::DocumentId;
    use crate::entry::{sign_and_encode, Entry, LogId};
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::{AsOperation, OperationEncoded};
    use crate::storage_provider::traits::test_utils::{
        test_db, EntryArgsRequest, EntryArgsResponse, PublishEntryRequest, PublishEntryResponse,
        SimplestStorageProvider, StorageEntry, StorageLog,
    };
    use crate::storage_provider::traits::{
        AsEntryArgsResponse, AsPublishEntryResponse, AsStorageEntry, AsStorageLog,
    };
    use crate::test_utils::fixtures::key_pair;

    use super::StorageProvider;

    #[async_trait]
    impl StorageProvider<StorageEntry, StorageLog> for SimplestStorageProvider {
        type EntryArgsRequest = EntryArgsRequest;

        type EntryArgsResponse = EntryArgsResponse;

        type PublishEntryRequest = PublishEntryRequest;

        type PublishEntryResponse = PublishEntryResponse;

        async fn get_document_by_entry(
            &self,
            entry_hash: &Hash,
        ) -> Result<Option<DocumentId>, Box<dyn std::error::Error>> {
            let entries = self.entries.lock().unwrap();

            let entry = entries.iter().find(|entry| entry.hash() == *entry_hash);

            let entry = match entry {
                Some(entry) => entry,
                None => return Ok(None),
            };

            let logs = self.logs.lock().unwrap();

            let log = logs
                .iter()
                .find(|log| log.id() == entry.log_id() && log.author() == entry.author());

            Ok(Some(log.unwrap().document_id()))
        }
    }

    #[rstest]
    #[async_std::test]
    async fn can_publish_entries(test_db: SimplestStorageProvider) {
        // Instantiate a new store
        let new_db = SimplestStorageProvider {
            logs: Arc::new(Mutex::new(Vec::new())),
            entries: Arc::new(Mutex::new(Vec::new())),
        };

        let entries = test_db.entries.lock().unwrap().clone();

        for entry in entries.clone() {
            // Publish each test entry in order
            let publish_entry_request = PublishEntryRequest(
                entry.entry_signed(),
                entry.operation_encoded().unwrap().clone(),
            );

            let publish_entry_response = new_db.publish_entry(&publish_entry_request).await;

            // Response should be ok
            assert!(publish_entry_response.is_ok());

            let mut seq_num = entry.seq_num();

            // If this is the highest entry in the db then break here, the test is over
            if seq_num.as_u64() == entries.len() as u64 {
                break;
            };

            // Calculate expected response
            let next_seq_num = seq_num.next().unwrap();
            let skiplink = entries
                .get(next_seq_num.as_u64() as usize - 1)
                .unwrap()
                .skiplink_hash();
            let backlink = entries
                .get(next_seq_num.as_u64() as usize - 1)
                .unwrap()
                .backlink_hash();
            let expected_reponse =
                PublishEntryResponse::new(backlink, skiplink, next_seq_num, LogId::default());

            // Response and expected response should match
            assert_eq!(publish_entry_response.unwrap(), expected_reponse);
        }
    }

    #[rstest]
    #[async_std::test]
    async fn gets_entry_args(test_db: SimplestStorageProvider) {
        // Instantiate a new store
        let new_db = SimplestStorageProvider {
            logs: Arc::new(Mutex::new(Vec::new())),
            entries: Arc::new(Mutex::new(Vec::new())),
        };

        let entries = test_db.entries.lock().unwrap().clone();

        for entry in entries.clone() {
            let is_create = entry.operation().is_create();

            // Determine document id
            let document_id: Option<DocumentId> = match is_create {
                true => None,
                false => Some(entries.get(0).unwrap().hash().into()),
            };

            // Construct entry args request
            let entry_args_request = EntryArgsRequest {
                author: entry.author(),
                document: document_id,
            };

            let entry_args_response = new_db.get_entry_args(&entry_args_request).await;

            // Response should be ok
            assert!(entry_args_response.is_ok());

            // Calculate expected response
            let seq_num = entry.seq_num();
            let backlink = entry.backlink_hash();
            let skiplink = entry.skiplink_hash();

            let expected_reponse =
                EntryArgsResponse::new(backlink, skiplink, seq_num, LogId::default());

            // Response and expected response should match
            assert_eq!(entry_args_response.unwrap(), expected_reponse);

            // Publish each test entry in order before next loop
            let publish_entry_request = PublishEntryRequest(
                entry.entry_signed(),
                entry.operation_encoded().unwrap().clone(),
            );

            new_db.publish_entry(&publish_entry_request).await.unwrap();
        }
    }

    #[rstest]
    #[async_std::test]
    async fn wrong_log_id(key_pair: KeyPair, test_db: SimplestStorageProvider) {
        // Instantiate a new store
        let new_db = SimplestStorageProvider {
            logs: Arc::new(Mutex::new(Vec::new())),
            entries: Arc::new(Mutex::new(Vec::new())),
        };

        let entries = test_db.entries.lock().unwrap().clone();

        // Entry request for valid first intry in log 1
        let publish_entry_request = PublishEntryRequest(
            entries.get(0).unwrap().entry_signed(),
            entries.get(0).unwrap().operation_encoded().unwrap(),
        );

        // Publish the first valid entry
        new_db.publish_entry(&publish_entry_request).await.unwrap();

        // Create a new entry with an invalid log id
        let entry_with_wrong_log_id = Entry::new(
            &LogId::new(2), // This is wrong!!
            Some(&entries.get(1).unwrap().operation()),
            entries.get(1).unwrap().skiplink_hash().as_ref(),
            entries.get(1).unwrap().backlink_hash().as_ref(),
            &entries.get(1).unwrap().seq_num(),
        )
        .unwrap();

        let signed_entry_with_wrong_log_id =
            sign_and_encode(&entry_with_wrong_log_id, &key_pair).unwrap();
        let encoded_operation =
            OperationEncoded::try_from(&entries.get(1).unwrap().operation()).unwrap();

        // Create request and publish invalid entry
        let request_with_wrong_log_id =
            PublishEntryRequest(signed_entry_with_wrong_log_id, encoded_operation);

        // Should error as the published entry contains an invalid log
        let error_response = new_db.publish_entry(&request_with_wrong_log_id).await;

        assert_eq!(
            format!("{}", error_response.unwrap_err()),
            "Requested log id 2 does not match expected log id 1"
        )
    }

    #[rstest]
    #[async_std::test]
    async fn document_does_not_exist(test_db: SimplestStorageProvider) {
        let entries = test_db.entries.lock().unwrap().clone();

        // Init database with one document missing it's CREATE entry
        let log_entries_without_document_root = vec![
            entries.get(1).unwrap().clone(),
            entries.get(2).unwrap().clone(),
            entries.get(3).unwrap().clone(),
            entries.get(4).unwrap().clone(),
        ];

        let new_db = SimplestStorageProvider {
            logs: Arc::new(Mutex::new(Vec::new())),
            entries: Arc::new(Mutex::new(log_entries_without_document_root)),
        };

        let entry = entries.get(6).unwrap();

        // Create request for publishing an entry which has a valid backlink and skiplink, but the
        // document it is associated with does not exist
        let publish_entry_with_non_existant_document =
            PublishEntryRequest(entry.entry_signed(), entry.operation_encoded().unwrap());

        let error_response = new_db
            .publish_entry(&publish_entry_with_non_existant_document)
            .await;

        assert_eq!(
            format!("{}", error_response.unwrap_err()),
            format!(
                "Could not find document hash for entry in database with id: {:?}",
                entry.hash()
            )
        )
    }

    #[rstest]
    #[async_std::test]
    async fn skiplink_does_not_exist(test_db: SimplestStorageProvider) {
        let entries = test_db.entries.lock().unwrap().clone();
        let logs = test_db.logs.lock().unwrap().clone();

        // Init database with on document log which has an entry at seq num 4 missing
        let log_entries_with_skiplink_missing = vec![
            entries.get(0).unwrap().clone(),
            entries.get(1).unwrap().clone(),
            entries.get(2).unwrap().clone(),
            entries.get(4).unwrap().clone(),
            entries.get(5).unwrap().clone(),
            entries.get(6).unwrap().clone(),
        ];

        let new_db = SimplestStorageProvider {
            logs: Arc::new(Mutex::new(logs)),
            entries: Arc::new(Mutex::new(log_entries_with_skiplink_missing)),
        };

        let entry = entries.get(7).unwrap();

        let publish_entry_request =
            PublishEntryRequest(entry.entry_signed(), entry.operation_encoded().unwrap());

        // Should error as an entry at seq num 8 should have a skiplink relation to the missing
        // entry at seq num 4
        let error_response = new_db.publish_entry(&publish_entry_request).await;

        assert_eq!(
            format!("{}", error_response.unwrap_err()),
            format!(
                "Could not find skiplink entry in database with id: {:?}",
                entry.hash()
            )
        )
    }
}
