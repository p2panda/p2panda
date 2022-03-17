// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::DocumentId;
use crate::entry::{decode_entry, SeqNum};
use crate::hash::Hash;
use crate::operation::{AsOperation, Operation};
use crate::storage_provider::errors::PublishEntryError;
use crate::storage_provider::models::{EntryWithOperation, Log};
use crate::storage_provider::traits::{
    AsEntryArgsRequest, AsEntryArgsResponse, AsPublishEntryRequest, AsPublishEntryResponse,
    AsStorageEntry, AsStorageLog, EntryStore, LogStore,
};
use crate::storage_provider::StorageProviderError;

/// Trait which handles all high level storage queries and insertions.
///
/// This trait should be implemented on the root storage provider struct. It's definitions
/// make up the the higher level methods a p2panda client needs for interacting with data
/// storage.
#[async_trait]
pub trait StorageProvider<StorageEntry: AsStorageEntry, StorageLog: AsStorageLog>:
    EntryStore<StorageEntry> + LogStore<StorageLog>
{
    /// Params when making a request to `get_entry_args`.
    type EntryArgsRequest: AsEntryArgsRequest + Sync;
    /// Response from a call to `get_entry_args`.
    type EntryArgsResponse: AsEntryArgsResponse;
    /// Params when making a request to `publish_entry`.
    type PublishEntryRequest: AsPublishEntryRequest + Sync;
    /// Response from a call to `publish_entry`.
    type PublishEntryResponse: AsPublishEntryResponse;

    /// Returns the related document for any entry.
    ///
    /// Every entry is part of a document and, through that, associated with a specific log id used
    /// by this document and author. This method returns that document id by looking up the log
    /// that the entry was stored in.
    async fn get_document_by_entry(
        &self,
        entry_hash: &Hash,
    ) -> Result<Option<DocumentId>, StorageProviderError>;

    /// Implementation of `panda_getEntryArguments` RPC method.
    ///
    /// Returns required data (backlink and skiplink entry hashes, last sequence number and the
    /// document's log_id) to encode a new bamboo entry.
    async fn get_entry_args(
        &self,
        params: &Self::EntryArgsRequest,
    ) -> Result<Self::EntryArgsResponse, StorageProviderError> {
        // Validate the entry args request parameters.
        params.validate()?;

        // Determine log_id for this document. If this is the very first operation in the document
        // graph, the `document` value is None and we will return the next free log id
        let log = self
            .find_document_log_id(params.author(), params.document().as_ref())
            .await?;

        // Determine backlink and skiplink hashes for the next entry. To do this we need the latest
        // entry in this log
        let entry_latest = self.latest_entry(params.author(), &log).await?;

        match entry_latest.clone() {
            // An entry was found which serves as the backlink for the upcoming entry
            Some(entry_backlink) => {
                let entry_hash_backlink = entry_backlink.entry_encoded().hash();
                // Determine skiplink ("lipmaa"-link) entry in this log
                let entry_hash_skiplink = self.determine_skiplink(&entry_latest.unwrap()).await?;

                Ok(Self::EntryArgsResponse::new(
                    Some(entry_hash_backlink),
                    entry_hash_skiplink,
                    *entry_backlink.entry_decoded().seq_num(),
                    *entry_backlink.entry_decoded().log_id(),
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

    /// Implementation of `panda_publishEntry` RPC method.
    ///
    /// Stores an author's Bamboo entry with operation payload in database after validating it.
    async fn publish_entry(
        &self,
        params: &Self::PublishEntryRequest,
    ) -> Result<Self::PublishEntryResponse, StorageProviderError> {
        // Create an `EntryWithOperation` which also validates the encoded entry and operation.
        let entry_with_operation = EntryWithOperation::new(
            params.entry_encoded().to_owned(),
            params.operation_encoded().to_owned(),
        )?;

        // Decode author, entry and operation. This conversion validates the operation hash
        let author = params.entry_encoded().author();
        let entry_encoded = params.entry_encoded();
        let entry = decode_entry(params.entry_encoded(), Some(params.operation_encoded()))?;
        let operation = Operation::from(params.operation_encoded());

        // Every operation refers to a document we need to determine. A document is identified by the
        // hash of its first `CREATE` operation, it is the root operation of every document graph
        let document_id = if operation.is_create() {
            // This is easy: We just use the entry hash directly to determine the document id
            DocumentId::new(entry_encoded.hash())
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
                .ok_or(PublishEntryError::OperationWithoutBacklink)?;

            self.get_document_by_entry(backlink_entry_hash)
                .await?
                .ok_or(PublishEntryError::DocumentMissing)?
        };

        // Determine expected log id for new entry
        let document_log_id = self
            .find_document_log_id(&author, Some(&document_id))
            .await?;

        // Check if provided log id matches expected log id
        if &document_log_id != entry.log_id() {
            return Err(PublishEntryError::InvalidLogId(
                entry.log_id().as_u64(),
                document_log_id.as_u64(),
            )
            .into());
        }

        // Get related bamboo backlink and skiplink entries
        let entry_backlink_bytes = if !entry.seq_num().is_first() {
            self.entry_at_seq_num(&author, entry.log_id(), &entry.seq_num_backlink().unwrap())
                .await?
                .map(|link| {
                    let bytes = link.entry_encoded().to_bytes();
                    Some(bytes)
                })
                .ok_or(PublishEntryError::BacklinkMissing)
        } else {
            Ok(None)
        }?;

        let entry_skiplink_bytes = if !entry.seq_num().is_first() {
            self.entry_at_seq_num(&author, entry.log_id(), &entry.seq_num_skiplink().unwrap())
                .await?
                .map(|link| {
                    let bytes = link.entry_encoded().to_bytes();
                    Some(bytes)
                })
                .ok_or(PublishEntryError::SkiplinkMissing)
        } else {
            Ok(None)
        }?;

        // Verify bamboo entry integrity, including encoding, signature of the entry correct back- and
        // skiplinks.
        bamboo_rs_core_ed25519_yasmf::verify(
            &entry_encoded.to_bytes(),
            Some(&params.operation_encoded().to_bytes()),
            entry_skiplink_bytes.as_deref(),
            entry_backlink_bytes.as_deref(),
        )?;

        // Register log in database when a new document is created
        if operation.is_create() {
            let log = Log::new(
                author.clone(),
                operation.schema(),
                document_id,
                *entry.log_id(),
            )
            .into();

            self.insert_log(log).await?;
        }

        let store_entry = StorageEntry::try_from(entry_with_operation)
            .map_err(|_| PublishEntryError::InvalidEntryWithOperation)?;

        // Finally insert Entry in database
        self.insert_entry(store_entry.clone()).await?;

        // Already return arguments for next entry creation
        let entry_latest: StorageEntry = self.latest_entry(&author, entry.log_id()).await?.unwrap();
        let entry_hash_skiplink = self.determine_skiplink(&entry_latest).await?;
        let next_seq_num = entry_latest
            .entry_decoded()
            .seq_num()
            .clone()
            .next()
            .unwrap();

        Ok(Self::PublishEntryResponse::new(
            Some(entry_encoded.hash()),
            entry_hash_skiplink,
            next_seq_num,
            *entry.log_id(),
        ))
    }
}
