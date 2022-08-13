// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;

use crate::document::error::DocumentBuilderError;
use crate::document::materialization::{build_graph, reduce};
use crate::document::{DocumentId, DocumentView, DocumentViewId};
use crate::identity::Author;
use crate::operation::traits::{AsOperation, AsVerifiedOperation};
use crate::operation::{OperationId, VerifiedOperation};
use crate::schema::SchemaId;
use crate::Human;

/// Flag to indicate if document was edited by at least one author.
pub type IsEdited = bool;

/// Flag to indicate if document was deleted by at least one author.
pub type IsDeleted = bool;

#[derive(Debug, Clone, Default)]
pub struct DocumentMeta {
    /// Flag indicating if document was deleted.
    deleted: IsDeleted,

    /// Flag indicating if document was edited.
    edited: IsEdited,

    /// List of operations this document consists of.
    operations: Vec<VerifiedOperation>,
}

/// A replicatable data type designed to handle concurrent updates in a way where all replicas
/// eventually resolve to the same deterministic value.
///
/// `Document`s contain a fixed set of sorted operations along with their resolved document view
/// and metadata. Documents are constructed by passing an unsorted collection of operations to
/// the `DocumentBuilder`.
#[derive(Debug, Clone)]
pub struct Document {
    id: DocumentId,
    view_id: DocumentViewId,
    author: Author,
    schema: SchemaId,
    view: Option<DocumentView>,
    meta: DocumentMeta,
}

impl Document {
    /// Get the document id.
    pub fn id(&self) -> &DocumentId {
        &self.id
    }

    /// Get the document view id.
    pub fn view_id(&self) -> &DocumentViewId {
        &self.view_id
    }

    /// Get the document author.
    pub fn author(&self) -> &Author {
        &self.author
    }

    /// Get the document schema.
    pub fn schema(&self) -> &SchemaId {
        &self.schema
    }

    /// Get the view of this document.
    pub fn view(&self) -> Option<&DocumentView> {
        self.view.as_ref()
    }

    /// Get the operations contained in this document.
    pub fn operations(&self) -> &Vec<VerifiedOperation> {
        &self.meta.operations
    }

    /// Returns true if this document has applied an UPDATE operation.
    pub fn is_edited(&self) -> IsEdited {
        self.meta.edited
    }

    /// Returns true if this document has processed a DELETE operation.
    pub fn is_deleted(&self) -> IsDeleted {
        self.meta.deleted
    }
}

impl Display for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl Human for Document {
    fn display(&self) -> String {
        let offset = yasmf_hash::MAX_YAMF_HASH_SIZE * 2 - 6;
        format!("<Document {}>", &self.id.as_str()[offset..])
    }
}

/// A struct for building [documents][`Document`] from a collection of [operations with
/// metadata][`crate::operation::VerifiedOperation`].
///
/// ## Example
///
/// ```
/// # extern crate p2panda_rs;
/// # #[cfg(test)]
/// # mod tests {
/// # use rstest::rstest;
/// # use p2panda_rs::document::DocumentBuilder;
/// # use p2panda_rs::operation::VerifiedOperation;
/// # use p2panda_rs::test_utils::meta_operation;
/// #
/// # #[rstest]
/// # fn main(#[from(meta_operation)] operation: VerifiedOperation) -> () {
/// // You need a `Vec<VerifiedOperation>` that includes the `CREATE` operation
/// let operations: Vec<VerifiedOperation> = vec![operation];
///
/// // Then you can make a `Document` from it
/// let document = DocumentBuilder::new(operations).build();
/// assert!(document.is_ok());
/// # }
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct DocumentBuilder {
    /// All the operations present in this document.
    operations: Vec<VerifiedOperation>,
}

impl DocumentBuilder {
    /// Instantiate a new `DocumentBuilder` from a collection of operations.
    pub fn new<O: AsVerifiedOperation>(operations: Vec<O>) -> DocumentBuilder {
        let operations = operations
            .iter()
            .map(|operation| VerifiedOperation {
                id: operation.id().to_owned(),
                version: operation.version(),
                action: operation.action(),
                schema_id: operation.schema_id(),
                previous_operations: operation.previous_operations(),
                fields: operation.fields(),
                public_key: operation.public_key().to_owned(),
            })
            .collect();

        Self { operations }
    }

    /// Get all operations for this document.
    pub fn operations(&self) -> Vec<VerifiedOperation> {
        self.operations.clone()
    }

    /// Validates all contained operations and builds the document.
    ///
    /// The returned document contains the latest resolved [document view][`DocumentView`].
    ///
    /// Validation checks the following:
    /// - There is exactly one `CREATE` operation.
    /// - All operations are causally connected to the root operation.
    /// - All operations follow the same schema.
    /// - No cycles exist in the graph.
    pub fn build(&self) -> Result<Document, DocumentBuilderError> {
        self.build_to_view_id(None)
    }

    /// Validates all contained operations and builds the document up to the
    /// requested [`DocumentViewId`].
    ///
    /// The returned document contains the requested [document view][`DocumentView`].
    ///
    /// Validation checks the following:
    /// - There is exactly one `CREATE` operation.
    /// - All operations are causally connected to the root operation.
    /// - All operations follow the same schema.
    /// - No cycles exist in the graph.
    pub fn build_to_view_id(
        &self,
        document_view_id: Option<DocumentViewId>,
    ) -> Result<Document, DocumentBuilderError> {
        // Find CREATE operation
        let mut collect_create_operation: Vec<VerifiedOperation> = self
            .operations()
            .into_iter()
            .filter(|op| op.is_create())
            .collect();

        // Check we have only one CREATE operation in the document
        let create_operation = match collect_create_operation.len() {
            0 => Err(DocumentBuilderError::NoCreateOperation),
            1 => Ok(collect_create_operation.pop().unwrap()),
            _ => Err(DocumentBuilderError::MoreThanOneCreateOperation),
        }?;

        // Get the document schema
        let schema = create_operation.schema_id();

        // Get the document author (or rather, the public key of the author who created this
        // document)
        let author = create_operation.public_key().to_owned();

        // Check all operations match the document schema
        let schema_error = self
            .operations()
            .iter()
            .any(|operation| operation.schema_id() != schema);

        if schema_error {
            return Err(DocumentBuilderError::OperationSchemaNotMatching);
        }

        let document_id = DocumentId::new(&create_operation.id().clone());

        // Build the graph.
        let mut graph = build_graph(&self.operations)?;

        // If a specific document view was requested then trim the graph to that point.
        if let Some(id) = document_view_id {
            graph = graph.trim(id.graph_tips())?;
        }

        // Topologically sort the operations in the graph.
        let sorted_graph_data = graph.sort()?;

        // These are the current graph tips, to be added to the document view id
        let graph_tips: Vec<OperationId> = sorted_graph_data
            .current_graph_tips()
            .iter()
            .map(|operation| operation.id().to_owned())
            .collect();

        // Reduce the sorted operations into a single key value map
        let (view, is_edited, is_deleted) = reduce(&sorted_graph_data.sorted()[..]);

        // Construct document meta data
        let meta = DocumentMeta {
            edited: is_edited,
            deleted: is_deleted,
            operations: sorted_graph_data.sorted(),
        };

        // Construct the document view id
        let document_view_id = DocumentViewId::new(&graph_tips);

        // Construct the document view, from the reduced values and the document view id
        let document_view = if is_deleted {
            None
        } else {
            // Unwrap as documents which aren't deleted will have a view
            Some(DocumentView::new(&document_view_id, &view.unwrap()))
        };

        Ok(Document {
            id: document_id,
            view_id: document_view_id,
            schema,
            author,
            view: document_view,
            meta,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use rstest::rstest;

    use crate::document::materialization::reduce;
    use crate::document::{DocumentId, DocumentViewFields, DocumentViewId, DocumentViewValue};
    use crate::identity::KeyPair;
    use crate::operation::traits::{AsOperation, AsVerifiedOperation};
    use crate::operation::{EncodedOperation, OperationId, OperationValue, VerifiedOperation};
    use crate::schema::SchemaId;
    use crate::test_utils::constants::SCHEMA_ID;
    use crate::test_utils::fixtures::{
        create_operation, delete_operation, operation, operation_fields, public_key,
        random_document_view_id, random_key_pair, random_operation_id, random_previous_operations,
        schema, update_operation, verified_operation, verified_operation_with_schema,
    };
    use crate::test_utils::mocks::{send_to_node, Client, Node};
    use crate::Human;

    use super::DocumentBuilder;

    #[rstest]
    fn string_representation(#[from(verified_operation)] operation: VerifiedOperation) {
        let builder = DocumentBuilder::new(vec![operation]);
        let document = builder.build().unwrap();

        assert_eq!(
            document.to_string(),
            "00206a28f82fc8d27671b31948117af7501a5a0de709b0cf9bc3586b67abe67ac29a"
        );

        // Short string representation
        assert_eq!(document.display(), "<Document 7ac29a>");

        // Make sure the id is matching
        assert_eq!(
            document.id().as_str(),
            "00206a28f82fc8d27671b31948117af7501a5a0de709b0cf9bc3586b67abe67ac29a"
        );
    }

    // @TODO: Refactor mock module first
    /* #[rstest]
    #[tokio::test]
    async fn resolve_documents(schema: SchemaId) {
        let panda = Client::new(
            "panda".to_string(),
            KeyPair::from_private_key_str(
                "ddcafe34db2625af34c8ba3cf35d46e23283d908c9848c8b43d1f5d0fde779ea",
            )
            .unwrap(),
        );
        let penguin = Client::new(
            "penguin".to_string(),
            KeyPair::from_private_key_str(
                "1c86b2524b48f0ba86103cddc6bdfd87774ab77ab4c0ea989ed0eeab3d28827a",
            )
            .unwrap(),
        );
        let mut node = Node::new();

        // Panda publishes a create operation.
        // This instantiates a new document.
        //
        // DOCUMENT: [panda_1]
        //
        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(&[("name", OperationValue::Text("Panda Cafe".to_string()))]),
        )
        .await
        .unwrap();

        // Panda publishes an update operation.
        // It contains the id of the previous operation in it's `previous_operations` array
        //
        // DOCUMENT: [panda_1]<--[panda_2]
        //
        let (panda_entry_2_hash, _) = send_to_node(
            &mut node,
            &panda,
            &update_operation(
                &[("name", OperationValue::Text("Panda Cafe!".to_string()))],
                &panda_entry_1_hash.clone().into(),
            ),
        )
        .await
        .unwrap();

        // Penguin publishes an update operation which creates a new branch in the graph.
        // This is because they didn't know about Panda's second operation.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]
        //                    \----[panda_2]
        let (penguin_entry_1_hash, _) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                &[("name", OperationValue::Text("Penguin Cafe!!!".to_string()))],
                &panda_entry_1_hash.clone().into(),
            ),
        )
        .await
        .unwrap();

        // Penguin publishes a new operation while now being aware of the previous branching situation.
        // Their `previous_operations` field now contains 2 operation id's.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]<---[penguin_2]
        //                    \----[panda_2]<--/
        let (penguin_entry_2_hash, _) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                &[("name", OperationValue::Text("Polar Bear Cafe".to_string()))],
                &DocumentViewId::new(&[
                    penguin_entry_1_hash.clone().into(),
                    panda_entry_2_hash.clone().into(),
                ])
                .unwrap(),
            ),
        )
        .await
        .unwrap();

        // Penguin publishes a new update operation which points at the current graph tip.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]<---[penguin_2]<--[penguin_3]
        //                    \----[panda_2]<--/
        let (penguin_entry_3_hash, _) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                &[(
                    "name",
                    OperationValue::Text("Polar Bear Cafe!!!!!!!!!!".to_string()),
                )],
                &penguin_entry_2_hash.clone().into(),
            ),
        )
        .await
        .unwrap();

        let operations: Vec<VerifiedOperation> = vec![
            node.operations()
                .get(&panda_entry_1_hash.into())
                .unwrap()
                .clone(),
            node.operations()
                .get(&panda_entry_2_hash.into())
                .unwrap()
                .clone(),
            node.operations()
                .get(&penguin_entry_1_hash.into())
                .unwrap()
                .clone(),
            node.operations()
                .get(&penguin_entry_2_hash.into())
                .unwrap()
                .clone(),
            node.operations()
                .get(&penguin_entry_3_hash.into())
                .unwrap()
                .clone(),
        ];

        let document = DocumentBuilder::new(operations.clone()).build();

        assert!(document.is_ok());

        let mut exp_result = DocumentViewFields::new();
        exp_result.insert(
            "name",
            DocumentViewValue::new(
                operations[4].id(),
                &OperationValue::Text("Polar Bear Cafe!!!!!!!!!!".to_string()),
            ),
        );

        let expected_graph_tips: Vec<OperationId> =
            vec![operations[4].clone().id().clone()];
        let expected_op_order = vec![
            operations[0].clone(),
            operations[2].clone(),
            operations[1].clone(),
            operations[3].clone(),
            operations[4].clone(),
        ];

        // Document should resolve to expected value

        let document = document.unwrap();
        assert_eq!(document.view().unwrap().get("name"), exp_result.get("name"));
        assert!(document.is_edited());
        assert!(!document.is_deleted());
        assert_eq!(document.author(), &panda.author());
        assert_eq!(document.schema(), &schema);
        assert_eq!(document.operations(), &expected_op_order);
        assert_eq!(document.view_id().graph_tips(), expected_graph_tips);
        assert_eq!(
            document.id(),
            &DocumentId::new(operations[0].id().to_owned())
        );

        // Multiple replicas receiving operations in different orders should resolve to same value.

        let replica_1 = DocumentBuilder::new(vec![
            operations[4].clone(),
            operations[3].clone(),
            operations[2].clone(),
            operations[1].clone(),
            operations[0].clone(),
        ])
        .build()
        .unwrap();

        let replica_2 = DocumentBuilder::new(vec![
            operations[2].clone(),
            operations[1].clone(),
            operations[0].clone(),
            operations[4].clone(),
            operations[3].clone(),
        ])
        .build()
        .unwrap();

        assert_eq!(
            replica_1.view().unwrap().get("name"),
            exp_result.get("name")
        );
        assert!(replica_1.is_edited());
        assert!(!replica_1.is_deleted());
        assert_eq!(replica_1.author(), &panda.author());
        assert_eq!(replica_1.schema(), &schema);
        assert_eq!(replica_1.operations(), &expected_op_order);
        assert_eq!(replica_1.view_id().graph_tips(), expected_graph_tips);
        assert_eq!(
            replica_1.id(),
            &DocumentId::new(operations[0].id().to_owned())
        );

        assert_eq!(
            replica_1.view().unwrap().get("name"),
            replica_2.view().unwrap().get("name")
        );
        assert_eq!(replica_1.id(), replica_2.id());
        assert_eq!(
            replica_1.view_id().graph_tips(),
            replica_2.view_id().graph_tips(),
        );
    } */

    // @TODO: Refactor mock module first
    /* #[rstest]
    #[tokio::test]
    async fn must_have_create_operation(#[from(random_key_pair)] key_pair_1: KeyPair) {
        let panda = Client::new("panda".to_string(), key_pair_1);
        let mut node = Node::new();

        // Panda publishes a create operation.
        // This instantiates a new document.
        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(&[("name", OperationValue::Text("Panda Cafe".to_string()))]),
        )
        .await
        .unwrap();

        // Panda publishes an update operation.
        // It contains the id of the previous operation in it's `previous_operations` array
        let (update_entry_hash, _) = send_to_node(
            &mut node,
            &panda,
            &update_operation(
                &[("name", OperationValue::Text("Panda Cafe!".to_string()))],
                &panda_entry_1_hash.into(),
            ),
        )
        .await
        .unwrap();

        // Only retrieve the update operation.
        let entries = node.entries();
        let only_the_update_operation = entries.get(&update_entry_hash).unwrap();

        let operations = vec![VerifiedOperation::new_from_entry(
            &only_the_update_operation.entry_signed(),
            &only_the_update_operation.operation_encoded().unwrap(),
        )
        .unwrap()];

        assert_eq!(
            DocumentBuilder::new(operations)
                .build()
                .unwrap_err()
                .to_string(),
            "Every document must contain one create operation".to_string()
        );
    } */

    // @TODO: Refactor mock module first
    /* #[rstest]
    #[tokio::test]
    async fn incorrect_previous_operations(
        #[from(random_key_pair)] key_pair_1: KeyPair,
        #[from(random_document_view_id)] incorrect_previous_operation: DocumentViewId,
    ) {
        let panda = Client::new("panda".to_string(), key_pair_1);
        let mut node = Node::new();

        // Panda publishes a create operation.
        // This instantiates a new document.
        let (panda_entry_1_hash, next_entry_args) = send_to_node(
            &mut node,
            &panda,
            &create_operation(&[("name", OperationValue::Text("Panda Cafe".to_string()))]),
        )
        .await
        .unwrap();

        // Construct an update operation with non-existant previous operations
        let operation_with_wrong_prev_ops = update_operation(
            &[("name", OperationValue::Text("Panda Cafe!".to_string()))],
            &incorrect_previous_operation,
        );

        let operation_one = node
            .operations()
            .get(&panda_entry_1_hash.into())
            .unwrap()
            .clone();

        let entry_two = panda.signed_encoded_entry(
            operation_with_wrong_prev_ops.clone(),
            &next_entry_args.log_id,
            next_entry_args.skiplink.as_ref(),
            next_entry_args.backlink.as_ref(),
            &next_entry_args.seq_num,
        );

        let operation_two = VerifiedOperation::new_from_entry(
            &entry_two,
            &OperationEncoded::try_from(&operation_with_wrong_prev_ops).unwrap(),
        )
        .unwrap();

        assert_eq!(
            DocumentBuilder::new(vec![operation_one, operation_two.clone()])
                .build()
                .unwrap_err()
                .to_string(),
            format!(
                "Operation {} cannot be connected to the document graph",
                operation_two.id()
            )
        );
    } */

    // @TODO: Refactor mock module first
    /* #[rstest]
    #[tokio::test]
    async fn operation_schemas_not_matching(
        #[from(random_key_pair)] key_pair_1: KeyPair,
        #[from(random_operation_id)] create_operation_id: OperationId,
        #[from(random_operation_id)] update_operation_id: OperationId,
    ) {
        let panda = Client::new("panda".to_string(), key_pair_1);

        let create_operation = VerifiedOperation::new(
            &panda.author(),
            &create_operation_id,
            &operation(
                Some(operation_fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe".to_string()),
                )])),
                None,
                Some(SchemaId::new("schema_field_definition_v1").unwrap()),
            ),
        )
        .unwrap();

        let update_operation = VerifiedOperation::new(
            &panda.author(),
            &update_operation_id,
            &operation(
                Some(operation_fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe!".to_string()),
                )])),
                Some(create_operation_id.into()),
                Some(SchemaId::new("schema_definition_v1").unwrap()),
            ),
        )
        .unwrap();

        assert_eq!(
            DocumentBuilder::new(vec![create_operation, update_operation])
                .build()
                .unwrap_err()
                .to_string(),
            "All operations in a document must follow the same schema".to_string()
        );
    } */

    // @TODO: Refactor mock module first
    /* #[rstest]
    #[tokio::test]
    async fn is_deleted(#[from(random_key_pair)] key_pair_1: KeyPair) {
        let panda = Client::new("panda".to_string(), key_pair_1);
        let mut node = Node::new();

        // Panda publishes a create operation.
        // This instantiates a new document.
        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(&[("name", OperationValue::Text("Panda Cafe".to_string()))]),
        )
        .await
        .unwrap();

        // Panda publishes an delete operation.
        // It contains the id of the previous operation in it's `previous_operations` array.
        send_to_node(
            &mut node,
            &panda,
            &delete_operation(&panda_entry_1_hash.into()),
        )
        .await
        .unwrap();

        let operations: Vec<VerifiedOperation> = node.operations().values().cloned().collect();
        let document = DocumentBuilder::new(operations).build().unwrap();

        assert!(document.is_deleted());

        assert!(document.view().is_none());
    } */

    // @TODO: Refactor mock module first
    /* #[rstest]
    #[tokio::test]
    async fn more_than_one_create(#[from(random_key_pair)] key_pair_1: KeyPair) {
        let panda = Client::new("panda".to_string(), key_pair_1);
        let mut node = Node::new();

        // Panda publishes a create operation.
        // This instantiates a new document.
        let (_panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(&[("name", OperationValue::Text("Panda Cafe".to_string()))]),
        )
        .await
        .unwrap();

        let create_verified_operation = node
            .operations()
            .values()
            .find(|operation| operation.is_create())
            .unwrap()
            .to_owned();

        assert_eq!(
            DocumentBuilder::new(vec![
                create_verified_operation.clone(),
                create_verified_operation
            ])
            .build()
            .unwrap_err()
            .to_string(),
            "Multiple create operations found".to_string()
        );
    } */

    // @TODO: Refactor mock module first
    /* #[rstest]
    #[tokio::test]
    async fn builds_specific_document_view() {
        let panda = Client::new(
            "panda".to_string(),
            KeyPair::from_private_key_str(
                "ddcafe34db2625af34c8ba3cf35d46e23283d908c9848c8b43d1f5d0fde779ea",
            )
            .unwrap(),
        );

        let mut node = Node::new();

        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(&[("name", OperationValue::Text("Panda Cafe".to_string()))]),
        )
        .await
        .unwrap();

        let (panda_entry_2_hash, _) = send_to_node(
            &mut node,
            &panda,
            &update_operation(
                &[("name", OperationValue::Text("Panda Cafe!".to_string()))],
                &panda_entry_1_hash.clone().into(),
            ),
        )
        .await
        .unwrap();

        let (panda_entry_3_hash, _) = send_to_node(
            &mut node,
            &panda,
            &update_operation(
                &[("name", OperationValue::Text("Panda Cafe!!!!!!".to_string()))],
                &panda_entry_1_hash.clone().into(),
            ),
        )
        .await
        .unwrap();

        // DOCUMENT: [panda_1]<--[penguin_1]
        //                    \----[panda_2]

        let operations: Vec<VerifiedOperation> = node.operations().values().cloned().collect();

        let document_builder = DocumentBuilder::new(operations);

        assert_eq!(
            document_builder
                .build_to_view_id(Some(panda_entry_1_hash.into()))
                .unwrap()
                .view()
                .unwrap()
                .get("name")
                .unwrap()
                .value(),
            &OperationValue::Text("Panda Cafe".to_string())
        );

        assert_eq!(
            document_builder
                .build_to_view_id(Some(panda_entry_2_hash.clone().into()))
                .unwrap()
                .view()
                .unwrap()
                .get("name")
                .unwrap()
                .value(),
            &OperationValue::Text("Panda Cafe!".to_string())
        );

        assert_eq!(
            document_builder
                .build_to_view_id(Some(panda_entry_3_hash.clone().into()))
                .unwrap()
                .view()
                .unwrap()
                .get("name")
                .unwrap()
                .value(),
            &OperationValue::Text("Panda Cafe!!!!!!".to_string())
        );

        assert_eq!(
            document_builder
                .build_to_view_id(Some(
                    DocumentViewId::new(&[panda_entry_2_hash.into(), panda_entry_3_hash.into()])
                        .unwrap()
                ))
                .unwrap()
                .view()
                .unwrap()
                .get("name")
                .unwrap()
                .value(),
            &OperationValue::Text("Panda Cafe!".to_string())
        );
    } */
}
