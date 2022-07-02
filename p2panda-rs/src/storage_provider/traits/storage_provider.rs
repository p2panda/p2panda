// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::DocumentId;
use crate::entry::SeqNum;
use crate::hash::Hash;
use crate::operation::{AsOperation, AsVerifiedOperation, Operation};
use crate::storage_provider::errors::PublishEntryError;
use crate::storage_provider::traits::{
    AsEntryArgsRequest, AsEntryArgsResponse, AsPublishEntryRequest, AsPublishEntryResponse,
    AsStorageEntry, AsStorageLog, EntryStore, LogStore, OperationStore,
};
use crate::Validate;

/// Trait which handles all high level storage queries and insertions.
///
/// This trait should be implemented on the root storage provider struct. It's definitions make up
/// the high level methods a p2panda client needs when interacting with data storage. It will be
/// used for storing entries (`publish_entry`), getting required entry arguments when creating
/// entries (`get_entry_args`) and all internal storage actions. Methods defined on `EntryStore`
/// and `LogStore` and `OperationStore` are for lower level access to their respective data
/// structures.
///
/// The methods defined here are the minimum required for a working storage backend, additional
/// custom methods can be added per implementation.
///
/// For example: if I wanted to use an SQLite backend, then I would first implement [`LogStore`]
/// and [`EntryStore`] traits with all their required methods defined (they are required traits
/// containing lower level accessors and setters for the respective data structures). With these
/// traits defined [`StorageProvider`] is almost complete as it contains default definitions for
/// most of it's methods (`get_entry_args` and `publish_entry` are defined below). The only one
/// which needs defining is `get_document_by_entry`. It is also possible to over-ride the default
/// definitions for any of the trait methods.
#[async_trait]
pub trait StorageProvider<
    StorageEntry: AsStorageEntry,
    StorageLog: AsStorageLog,
    StorageOperation: AsVerifiedOperation,
>: EntryStore<StorageEntry> + LogStore<StorageLog> + OperationStore<StorageOperation>
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
    ///
    /// If the passed entry cannot be found, or it's associated document doesn't exist yet, `None`
    /// is returned.
    async fn get_document_by_entry(
        &self,
        entry_hash: &Hash,
    ) -> Result<Option<DocumentId>, Box<dyn std::error::Error + Send + Sync>>;

    /// Returns required data (backlink and skiplink entry hashes, last sequence number and the
    /// document's log_id) to encode a new bamboo entry.
    async fn get_entry_args(
        &self,
        params: &Self::EntryArgsRequest,
    ) -> Result<Self::EntryArgsResponse, Box<dyn std::error::Error + Send + Sync>> {
        // Validate the entry args request parameters.
        params.validate()?;

        // Determine log_id for this document. If this is the very first operation in the document
        // graph, the `document` value is None and we will return the next free log id
        let log = self
            .find_document_log_id(params.author(), params.document_id().as_ref())
            .await?;

        // Determine backlink and skiplink hashes for the next entry. To do this we need the latest
        // entry in this log
        let entry_latest = self.get_latest_entry(params.author(), &log).await?;

        match entry_latest.clone() {
            // An entry was found which serves as the backlink for the upcoming entry
            Some(entry_backlink) => {
                let entry_latest = entry_latest.unwrap();
                let entry_hash_backlink = entry_backlink.hash();
                // Determine skiplink ("lipmaa"-link) entry in this log
                let entry_hash_skiplink = self.determine_next_skiplink(&entry_latest).await?;

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
    ) -> Result<Self::PublishEntryResponse, Box<dyn std::error::Error + Send + Sync>> {
        // Create a storage entry.
        let entry = StorageEntry::new(params.entry_signed(), params.operation_encoded())?;
        // Validate the entry (this also maybe happened in the above constructor)
        entry.validate()?;

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
            // We can determine the used document hash by looking at this operations' previous_operations.
            let operation = Operation::from(params.operation_encoded());

            operation.validate()?;

            // Unwrap here as we validated in the previous line which would error if previous_operations wasn't present.
            let previous_operation_id = operation
                .previous_operations()
                .unwrap()
                .into_iter()
                .next()
                // Unwrap as all DocumentViewId's contain at least one OperationId.
                .unwrap();

            self.get_document_by_entry(previous_operation_id.as_hash())
                .await?
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
        let entry_backlink_bytes = self
            .try_get_backlink(&entry)
            .await?
            .map(|link| link.entry_bytes());

        let entry_skiplink_bytes = self
            .try_get_skiplink(&entry)
            .await?
            .map(|link| link.entry_bytes());

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
            let log = StorageLog::new(
                &entry.author(),
                &entry.operation().schema(),
                &document_id,
                &entry.log_id(),
            );

            self.insert_log(log).await?;
        }

        // Finally insert Entry in database
        self.insert_entry(entry.clone()).await?;

        // Already return arguments for next entry creation
        let entry_latest: StorageEntry = self
            .get_latest_entry(&entry.author(), &entry.log_id())
            .await?
            .unwrap();
        let entry_hash_skiplink = self.determine_next_skiplink(&entry_latest).await?;
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

    use rstest::rstest;

    use crate::document::DocumentId;
    use crate::entry::{sign_and_encode, Entry, LogId};
    use crate::identity::KeyPair;
    use crate::operation::{
        AsOperation, OperationEncoded, OperationFields, OperationId, OperationValue,
    };
    use crate::storage_provider::traits::test_utils::{test_db, TestStore};
    use crate::storage_provider::traits::{
        AsEntryArgsResponse, AsPublishEntryResponse, AsStorageEntry,
    };
    use crate::test_utils::db::{
        EntryArgsRequest, EntryArgsResponse, PublishEntryRequest, PublishEntryResponse,
        SimplestStorageProvider,
    };
    use crate::test_utils::fixtures::{entry, key_pair, operation, operation_fields, operation_id};

    use super::StorageProvider;

    #[rstest]
    #[async_std::test]
    async fn can_publish_entries(
        #[from(test_db)]
        #[with(20, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        // Instantiate a new store
        let new_db = SimplestStorageProvider::default();

        let entries = db.store.entries.lock().unwrap().clone();

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
    async fn rejects_invalid_backlink(
        key_pair: KeyPair,
        #[from(test_db)]
        #[with(4, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let new_db = SimplestStorageProvider::default();

        let entries = db.store.entries.lock().unwrap().clone();

        // Publish 3 entries to the new database
        for index in 0..3 {
            let entry = entries.get(index).unwrap();
            let publish_entry_request = PublishEntryRequest(
                entry.entry_signed(),
                entry.operation_encoded().unwrap().clone(),
            );

            new_db.publish_entry(&publish_entry_request).await.unwrap();
        }

        // Retrieve the forth entry
        let entry_four = entries.get(3).unwrap();

        // Reconstruct it with an invalid backlink
        let entry_with_invalid_backlink = Entry::new(
            &entry_four.log_id(),
            Some(&entry_four.operation()),
            entry_four.skiplink_hash().as_ref(),
            Some(&entries.get(1).unwrap().hash()),
            &entry_four.seq_num(),
        )
        .unwrap();

        let entry_signed = sign_and_encode(&entry_with_invalid_backlink, &key_pair).unwrap();

        let publish_entry_request = PublishEntryRequest(
            entry_signed.clone(),
            entry_four.operation_encoded().unwrap(),
        );

        let error_response = new_db.publish_entry(&publish_entry_request).await;

        println!("{:#?}", error_response);
        assert_eq!(
            format!("{}", error_response.unwrap_err()),
            format!(
                "The backlink hash encoded in the entry: {} did not match the expected backlink hash",
                entry_signed.hash()
            )
        )
    }

    #[rstest]
    #[async_std::test]
    async fn rejects_invalid_skiplink(
        key_pair: KeyPair,
        #[from(test_db)]
        #[with(4, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let new_db = SimplestStorageProvider::default();

        let entries = db.store.entries.lock().unwrap().clone();

        // Publish 3 entries to the new database
        for index in 0..3 {
            let entry = entries.get(index).unwrap();
            let publish_entry_request = PublishEntryRequest(
                entry.entry_signed(),
                entry.operation_encoded().unwrap().clone(),
            );

            new_db.publish_entry(&publish_entry_request).await.unwrap();
        }

        // Retrieve the forth entry
        let entry_four = entries.get(3).unwrap();

        // Reconstruct it with an invalid skiplink
        let entry_with_invalid_backlink = Entry::new(
            &entry_four.log_id(),
            Some(&entry_four.operation()),
            Some(&entries.get(1).unwrap().hash()),
            entry_four.backlink_hash().as_ref(),
            &entry_four.seq_num(),
        )
        .unwrap();

        let entry_signed = sign_and_encode(&entry_with_invalid_backlink, &key_pair).unwrap();

        let publish_entry_request = PublishEntryRequest(
            entry_signed.clone(),
            entry_four.operation_encoded().unwrap(),
        );

        let error_response = new_db.publish_entry(&publish_entry_request).await;

        println!("{:#?}", error_response);
        assert_eq!(
            format!("{}", error_response.unwrap_err()),
            format!(
                "The skiplink hash encoded in the entry: {} did not match the known hash of the skiplink target",
                entry_signed.hash()
            )
        )
    }

    #[rstest]
    #[async_std::test]
    async fn gets_entry_args(
        #[from(test_db)]
        #[with(20, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        // Instantiate a new store
        let new_db = SimplestStorageProvider::default();

        let entries = db.store.entries.lock().unwrap().clone();

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
    async fn wrong_log_id(
        key_pair: KeyPair,
        #[from(test_db)]
        #[with(2, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        // Instantiate a new store
        let new_db = SimplestStorageProvider::default();

        let entries = db.store.entries.lock().unwrap().clone();

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
    async fn skiplink_does_not_exist(
        #[from(test_db)]
        #[with(8, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let entries = db.store.entries.lock().unwrap().clone();
        let logs = db.store.logs.lock().unwrap().clone();

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
            operations: Arc::new(Mutex::new(Vec::new())),
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
                "Could not find expected skiplink in database for entry with id: {}",
                entry.hash()
            )
        )
    }

    #[rstest]
    #[async_std::test]
    async fn prev_op_does_not_exist(
        #[from(test_db)]
        #[with(4, 1)]
        #[future]
        db: TestStore,
        operation_fields: OperationFields,
        #[from(operation_id)] invalid_prev_op: OperationId,
        key_pair: KeyPair,
    ) {
        let db = db.await;
        let entries = db.store.entries.lock().unwrap().clone();
        let logs = db.store.logs.lock().unwrap().clone();

        // Init database with 3 valid entries
        let three_valid_entries = vec![
            entries.get(0).unwrap().clone(),
            entries.get(1).unwrap().clone(),
            entries.get(2).unwrap().clone(),
        ];

        let new_db = SimplestStorageProvider {
            logs: Arc::new(Mutex::new(logs)),
            entries: Arc::new(Mutex::new(three_valid_entries)),
            operations: Arc::new(Mutex::new(Vec::new())),
        };

        // Get the valid next entry
        let next_entry = entries.get(3).unwrap();

        // Recreate this entry and replace previous_operations to contain invalid OperationId
        let update_operation_with_invalid_previous_operations = operation(
            Some(operation_fields.clone()),
            Some(invalid_prev_op.into()),
            None,
        );

        let update_entry = entry(
            next_entry.seq_num().as_u64(),
            next_entry.log_id().as_u64(),
            next_entry.backlink_hash(),
            next_entry.skiplink_hash(),
            Some(update_operation_with_invalid_previous_operations.clone()),
        );

        let encoded_entry = sign_and_encode(&update_entry, &key_pair).unwrap();
        let encoded_operation =
            OperationEncoded::try_from(&update_operation_with_invalid_previous_operations).unwrap();

        // Publish this entry (which contains an invalid previous_operation)
        let publish_entry_request = PublishEntryRequest(encoded_entry.clone(), encoded_operation);

        let error_response = new_db.publish_entry(&publish_entry_request).await;

        assert_eq!(
            format!("{}", error_response.unwrap_err()),
            format!(
                "Could not find document for entry in database with id: {}",
                encoded_entry.hash()
            )
        )
    }

    #[rstest]
    #[async_std::test]
    async fn invalid_entry_op_pair(
        #[from(test_db)]
        #[with(4, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let entries = db.store.entries.lock().unwrap().clone();
        let logs = db.store.logs.lock().unwrap().clone();

        // Init database with 3 valid entries
        let three_valid_entries = vec![
            entries.get(0).unwrap().clone(),
            entries.get(1).unwrap().clone(),
            entries.get(2).unwrap().clone(),
        ];

        let new_db = SimplestStorageProvider {
            logs: Arc::new(Mutex::new(logs)),
            entries: Arc::new(Mutex::new(three_valid_entries)),
            operations: Arc::new(Mutex::new(Vec::new())),
        };

        // Get the valid next entry
        let next_entry = entries.get(3).unwrap();

        // Create a new operation which does not match the one contained in the entry hash
        let mismatched_operation = operation(
            Some(operation_fields(vec![(
                "poopy",
                OperationValue::Text("This is the WRONG operation :-(".to_string()),
            )])),
            Some(next_entry.operation_encoded().unwrap().hash().into()),
            None,
        );

        let encoded_operation = OperationEncoded::try_from(&mismatched_operation).unwrap();

        // Publish this entry with an mismatching operation
        let publish_entry_request =
            PublishEntryRequest(next_entry.entry_signed(), encoded_operation);

        let error_response = new_db.publish_entry(&publish_entry_request).await;

        assert_eq!(
            format!("{}", error_response.unwrap_err()),
            "operation needs to match payload hash of encoded entry"
        )
    }
}
