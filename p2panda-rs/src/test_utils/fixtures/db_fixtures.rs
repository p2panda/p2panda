// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest::fixture;

use crate::operation_v2::OperationValue;
use crate::schema::Schema;
use crate::test_utils::constants;
use crate::test_utils::fixtures::schema;
use crate::test_utils::memory_store::helpers::PopulateStoreConfig;

/// Fixture for passing `PopulateStoreConfig` into tests.
#[fixture]
pub fn populate_store_config(
    // Number of operations per document
    #[default(0)] no_of_operations: usize,
    // Number of documents for each public key
    #[default(0)] no_of_documents: usize,
    // Number of public keys, each with documents populated as defined above
    #[default(0)] no_of_public_keys: usize,
    // A boolean flag for whether all documents should contain a delete operation
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
) -> PopulateStoreConfig {
    PopulateStoreConfig {
        no_of_operations,
        no_of_documents,
        no_of_public_keys,
        with_delete,
        schema,
        create_operation_fields,
        update_operation_fields,
    }
}
