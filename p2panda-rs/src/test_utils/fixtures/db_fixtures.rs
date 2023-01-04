// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest::fixture;

use crate::operation::OperationValue;
use crate::schema::Schema;
use crate::test_utils::constants;
use crate::test_utils::db::MemoryStore;
use crate::test_utils::db::test_db::{PopulateDatabaseConfig, TestDatabase, populate_store, TestData};
use crate::test_utils::fixtures::schema;

/// Fixture for passing `PopulateDatabaseConfig` into tests.
#[fixture]
pub fn test_db_config(
    // Number of entries per log/document
    #[default(0)] no_of_entries: usize,
    // Number of logs for each public key
    #[default(0)] no_of_logs: usize,
    // Number of public keys, each with logs populated as defined above
    #[default(0)] no_of_public_keys: usize,
    // A boolean flag for whether all logs should contain a delete operation
    #[default(false)] with_delete: bool,
    // The schema used for all operations in the db
    #[from(schema)] schema: Schema,
    // The fields used for every CREATE operation
    #[default(constants::test_fields())] create_operation_fields: Vec<(
        &'static str,
        OperationValue,
    )>,
    // The fields used for every UPDATE operation
    #[default(constants::test_fields())] update_operation_fields: Vec<(
        &'static str,
        OperationValue,
    )>,
) -> PopulateDatabaseConfig {
    PopulateDatabaseConfig {
        no_of_entries,
        no_of_logs,
        no_of_public_keys,
        with_delete,
        schema,
        create_operation_fields,
        update_operation_fields,
    }
}

/// Fixture for constructing a storage provider instance backed by a pre-populated database.
///
/// Passed parameters define what the database should contain. The first entry in each log contains
/// a valid CREATE operation following entries contain UPDATE operations. If the with_delete
/// flag is set to true the last entry in all logs contain be a DELETE operation.
#[fixture]
pub async fn test_db(
    // Number of entries per log/document
    #[default(0)] no_of_entries: usize,
    // Number of logs for each public key
    #[default(0)] no_of_logs: usize,
    // Number of public keys, each with logs populated as defined above
    #[default(0)] no_of_public_keys: usize,
    // A boolean flag for wether all logs should contain a delete operation
    #[default(false)] with_delete: bool,
    // The schema used for all operations in the db
    #[from(schema)] schema: Schema,
    // The fields used for every CREATE operation
    #[default(constants::test_fields())] create_operation_fields: Vec<(
        &'static str,
        OperationValue,
    )>,
    // The fields used for every UPDATE operation
    #[default(constants::test_fields())] update_operation_fields: Vec<(
        &'static str,
        OperationValue,
    )>,
) -> TestDatabase {
    let config = PopulateDatabaseConfig {
        no_of_entries,
        no_of_logs,
        no_of_public_keys,
        with_delete,
        schema,
        create_operation_fields,
        update_operation_fields,
    };

    let store = MemoryStore::default();
    let (key_pairs, documents) = populate_store(&store, &config).await;
    TestDatabase::new(
        &store,
        TestData {
            key_pairs,
            documents,
        },
    )
}
