// SPDX-License-Identifier: AGPL-3.0-or-later

//! Helper methods for working with a storage provider when testing.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::api::publish;
use crate::document::{DocumentId, DocumentViewId};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::operation::body::encode::encode_body;
use crate::operation::header::encode::encode_header;
use crate::operation::traits::Identifiable;
use crate::operation::{Operation, OperationBuilder, OperationValue};
use crate::schema::Schema;
use crate::storage_provider::traits::OperationStore;
use crate::storage_provider::utils::Result;
use crate::test_utils::constants;

/// Configuration used when populating the store for testing.
#[derive(Debug)]
pub struct PopulateStoreConfig {
    /// Number of entries per log/document.
    pub no_of_operations: usize,

    /// Number of logs for each public key.
    pub no_of_documents: usize,

    /// Number of public keys, each with logs populated as defined above.
    pub no_of_public_keys: usize,

    /// A boolean flag for wether all logs should contain a delete operation.
    pub with_delete: bool,

    /// The schema used for all operations in the db.
    pub schema: Schema,

    /// The fields used for every CREATE operation.
    pub create_operation_fields: Vec<(&'static str, OperationValue)>,

    /// The fields used for every UPDATE operation.
    pub update_operation_fields: Vec<(&'static str, OperationValue)>,
}

impl Default for PopulateStoreConfig {
    fn default() -> Self {
        Self {
            no_of_operations: 0,
            no_of_documents: 0,
            no_of_public_keys: 0,
            with_delete: false,
            schema: constants::schema(),
            create_operation_fields: constants::test_fields(),
            update_operation_fields: constants::test_fields(),
        }
    }
}

/// Helper for creating many key_pairs.
///
/// If there is only one key_pair in the list it will always be the default testing
/// key pair.
pub fn many_key_pairs(no_of_public_keys: usize) -> Vec<KeyPair> {
    let mut key_pairs = Vec::new();
    match no_of_public_keys {
        0 => (),
        1 => key_pairs.push(KeyPair::from_private_key_str(constants::PRIVATE_KEY).unwrap()),
        _ => {
            key_pairs.push(KeyPair::from_private_key_str(constants::PRIVATE_KEY).unwrap());
            for _index in 1..no_of_public_keys {
                key_pairs.push(KeyPair::new())
            }
        }
    };
    key_pairs
}

/// Helper method for populating the store with test data.
///
/// Passed parameters define what the store should contain. The first entry in each log contains a
/// valid CREATE operation following entries contain UPDATE operations. If the with_delete flag is set
/// to true the last entry in all logs contain be a DELETE operation.
pub async fn populate_store<S: OperationStore>(
    store: &S,
    config: &PopulateStoreConfig,
) -> (Vec<KeyPair>, Vec<DocumentId>) {
    let key_pairs = many_key_pairs(config.no_of_public_keys);
    let mut documents: Vec<DocumentId> = Vec::new();
    for key_pair in &key_pairs {
        // safely unwrap as we expect all times to be greater than "1970-01-01 00:00:00 UTC"
        let mut timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        for _ in 0..config.no_of_documents {
            let mut backlink: Option<Hash> = None;
            let mut previous: Option<DocumentViewId> = None;
            let mut document_id: Option<DocumentId> = None;

            // We increment the timestamp for each document so that they each have a unique hash
            // for their document id.
            timestamp += 1;

            for index in 0..config.no_of_operations {
                let mut operation_builder =
                    OperationBuilder::new(config.schema.id(), timestamp).seq_num(index as u64);

                operation_builder = match index {
                    // CREATE operation
                    0 => operation_builder.fields(&config.create_operation_fields),
                    // UPDATE and DELETE operations
                    _ => {
                        operation_builder = operation_builder
                            .document_id(&document_id.clone().expect("document_id should be set"))
                            .backlink(&backlink.expect("backlink should be set"))
                            .previous(&previous.expect("previous should be set"));

                        if index == (config.no_of_operations - 1) && config.with_delete {
                            // additionally set action on DELETE operations
                            operation_builder = operation_builder.tombstone();
                        } else {
                            // otherwise set fields on UPDATE operations
                            operation_builder =
                                operation_builder.fields(&config.update_operation_fields);
                        }
                        operation_builder
                    }
                };

                // build and sign the operation.
                let operation = operation_builder
                    .sign(&key_pair)
                    .expect("can build operation");

                store
                    .insert_operation(&operation)
                    .await
                    .expect("can insert operation");

                // Set the previous and backlink based on current operation id
                backlink = Some(operation.id().clone().into());
                previous = Some(operation.id().clone().into());

                if index == 0 {
                    // Push this new document id to the documents vec and set current document id.
                    let new_document_id = DocumentId::new(&operation.id());
                    document_id = Some(new_document_id.clone());
                    documents.push(new_document_id);
                }
            }
        }
    }
    (key_pairs, documents)
}

/// Helper method for publishing an operation encoded on an entry to a store.
pub async fn send_to_store<S: OperationStore>(
    store: &S,
    operation: &Operation,
    schema: &Schema,
) -> Result<()> {
    // Encode the header.
    let encoded_header = encode_header(operation.header())?;

    // Encode the body.
    let encoded_body = encode_body(operation.body())?;

    // Publish the entry and get the next entry args.
    publish(
        store,
        schema,
        &encoded_header,
        &operation.body().into(),
        &encoded_body,
    )
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::hash::HashId;
    use crate::identity::KeyPair;
    use crate::operation::traits::Identifiable;
    use crate::operation::{Operation, OperationBuilder, OperationValue};
    use crate::schema::Schema;
    use crate::storage_provider::traits::DocumentStore;
    use crate::test_utils::constants::TIMESTAMP;
    use crate::test_utils::fixtures::{key_pair, operation_fields, populate_store_config, schema};
    use crate::test_utils::memory_store::helpers::{
        populate_store, send_to_store, PopulateStoreConfig,
    };
    use crate::test_utils::memory_store::MemoryStore;

    #[rstest]
    #[tokio::test]
    async fn correct_test_values(
        schema: Schema,
        #[from(populate_store_config)]
        #[with(10, 4, 2)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        let (key_pairs, documents) = populate_store(&store, &config).await;

        assert_eq!(key_pairs.len(), 2);
        assert_eq!(documents.len(), 8);
        assert_eq!(store.operations.lock().unwrap().len(), 80);
        assert_eq!(
            store
                .get_documents_by_schema(schema.id())
                .await
                .unwrap()
                .len(),
            8
        );
    }

    #[rstest]
    #[tokio::test]
    async fn sends_to_store(
        schema: Schema,
        #[from(operation_fields)] fields: Vec<(&str, OperationValue)>,
        #[from(key_pair)] key_pair_1: KeyPair,
    ) {
        let store = MemoryStore::default();

        let create_operation = OperationBuilder::new(schema.id(), TIMESTAMP)
            .fields(&fields)
            .sign(&key_pair_1)
            .unwrap();

        // Publish a create operation.
        send_to_store(&store, &create_operation, &schema)
            .await
            .unwrap();

        let update_operation: Operation = OperationBuilder::new(schema.id(), TIMESTAMP + 1)
            .document_id(&create_operation.id().clone().into())
            .backlink(create_operation.id().as_hash())
            .previous(&create_operation.id().clone().into())
            .seq_num(1)
            .fields(&fields)
            .sign(&key_pair_1)
            .unwrap();

        // Publish an update operation.
        send_to_store(&store, &update_operation, &schema)
            .await
            .unwrap();
    }
}
