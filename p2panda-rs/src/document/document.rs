// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;

use crate::document::{
    DocumentBuilderError, DocumentId, DocumentView, DocumentViewFields, DocumentViewId,
    DocumentViewValue,
};
use crate::graph::Graph;
use crate::identity::Author;
use crate::operation::{AsOperation, AsVerifiedOperation, OperationId, VerifiedOperation};
use crate::schema::SchemaId;

/// Construct a graph from a list of operations.
pub(super) fn build_graph(
    operations: &[VerifiedOperation],
) -> Result<Graph<OperationId, VerifiedOperation>, DocumentBuilderError> {
    let mut graph = Graph::new();

    // Add all operations to the graph.
    for operation in operations {
        graph.add_node(operation.operation_id(), operation.clone());
    }

    // Add links between operations in the graph.
    for operation in operations {
        if let Some(previous_operations) = operation.previous_operations() {
            for previous in previous_operations {
                let success = graph.add_link(&previous, operation.operation_id());
                if !success {
                    return Err(DocumentBuilderError::InvalidOperationLink(
                        operation.operation_id().to_owned(),
                    ));
                }
            }
        }
    }

    Ok(graph)
}

type IsEdited = bool;
type IsDeleted = bool;

/// Reduce a list of operations into a single view.
///
/// Returns the reduced fields of a document view along with the `edited` and `deleted` boolean
/// flags. If the document contains a DELETE operation, then no view is returned and the `deleted`
/// flag is set to true. If the document contains one or more UPDATE operations, then the reduced
/// view is returned and the `edited` flag is set to true.
pub(super) fn reduce(
    ordered_operations: &[VerifiedOperation],
) -> (Option<DocumentViewFields>, IsEdited, IsDeleted) {
    let mut is_edited = false;

    let mut document_view_fields = DocumentViewFields::new();

    for operation in ordered_operations {
        if operation.is_delete() {
            return (None, true, true);
        }

        if operation.is_update() {
            is_edited = true
        }

        if let Some(fields) = operation.fields() {
            for (key, value) in fields.iter() {
                let document_view_value = DocumentViewValue::new(operation.operation_id(), value);
                document_view_fields.insert(key, document_view_value);
            }
        }
    }

    (Some(document_view_fields), is_edited, false)
}

#[derive(Debug, Clone, Default)]
pub struct DocumentMeta {
    deleted: IsDeleted,
    edited: IsEdited,
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
        write!(f, "<Document {}>", self.id)
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
    pub fn new(operations: Vec<VerifiedOperation>) -> DocumentBuilder {
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
        let schema = create_operation.schema();

        // Get the document author (or rather, the public key of the author who created this
        // document)
        let author = create_operation.public_key().to_owned();

        // Check all operations match the document schema
        let schema_error = self
            .operations()
            .iter()
            .any(|operation| operation.schema() != schema);

        if schema_error {
            return Err(DocumentBuilderError::OperationSchemaNotMatching);
        }

        let document_id = DocumentId::new(create_operation.operation_id().clone());

        // Build the graph.
        let mut graph = build_graph(&self.operations)?;

        // If a specific document view was requested then trim the graph to that point.
        match document_view_id {
            Some(id) => graph = graph.trim(&id.sorted())?,
            None => (),
        };

        // Topologically sort the operations in the graph.
        let sorted_graph_data = graph.sort()?;

        // These are the current graph tips, to be added to the document view id
        let graph_tips: Vec<OperationId> = sorted_graph_data
            .current_graph_tips()
            .iter()
            .map(|operation| operation.operation_id().to_owned())
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
        let document_view_id = DocumentViewId::new(&graph_tips).unwrap();

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

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use rstest::rstest;

    use crate::document::document_view_fields::{DocumentViewFields, DocumentViewValue};
    use crate::document::{DocumentId, DocumentViewId};
    use crate::identity::KeyPair;
    use crate::operation::{
        AsVerifiedOperation, OperationEncoded, OperationId, OperationValue, VerifiedOperation,
    };
    use crate::schema::{FieldType, SchemaId};
    use crate::test_utils::constants::TEST_SCHEMA_ID;
    use crate::test_utils::fixtures::{
        create_operation, delete_operation, operation, operation_fields, random_document_view_id,
        random_key_pair, random_previous_operations, schema, schema_item, update_operation,
        verified_operation,
    };
    use crate::test_utils::mocks::{send_to_node, Client, Node};

    use super::{reduce, DocumentBuilder};

    #[rstest]
    fn reduces_operations(
        #[from(verified_operation)] create_operation: VerifiedOperation,
        #[from(verified_operation)]
        #[with(
            Some(operation_fields(vec![("username", OperationValue::Text("Yahooo!".into()))])), Some(random_previous_operations(1)))
        ]
        update_operation: VerifiedOperation,
        #[from(verified_operation)]
        #[with(None, Some(random_previous_operations(1)))]
        delete_operation: VerifiedOperation,
    ) {
        let (reduced_create, is_edited, is_deleted) = reduce(&[create_operation.clone()]);
        assert_eq!(
            *reduced_create.unwrap().get("username").unwrap().value(),
            OperationValue::Text("bubu".to_string())
        );
        assert!(!is_edited);
        assert!(!is_deleted);

        let (reduced_update, is_edited, is_deleted) =
            reduce(&[create_operation.clone(), update_operation.clone()]);
        assert_eq!(
            *reduced_update.unwrap().get("username").unwrap().value(),
            OperationValue::Text("Yahooo!".to_string())
        );
        assert!(is_edited);
        assert!(!is_deleted);

        let (reduced_delete, is_edited, is_deleted) =
            reduce(&[create_operation, update_operation, delete_operation]);
        // The value remains the same, but the deleted flag is true now.
        assert!(reduced_delete.is_none());
        assert!(is_edited);
        assert!(is_deleted);
    }

    #[rstest]
    fn resolve_documents(schema: SchemaId) {
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
        let test_schema = schema_item(schema.clone(), "", vec![("name", FieldType::String)]);
        let mut node = Node::new(vec![test_schema.clone()]);

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
                &[("name", OperationValue::Text("Penguin Cafe".to_string()))],
                &panda_entry_1_hash.clone().into(),
            ),
        )
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
        .unwrap();

        let operations: Vec<VerifiedOperation> = [
            panda_entry_1_hash,
            panda_entry_2_hash,
            penguin_entry_1_hash,
            penguin_entry_2_hash,
            penguin_entry_3_hash,
        ]
        .iter()
        .map(|hash| {
            let entry = node.get_entry(hash);
            VerifiedOperation::new_from_entry(
                &entry.entry_encoded(),
                &entry.operation_encoded(),
                &test_schema,
            )
            .unwrap()
        })
        .collect();

        let document = DocumentBuilder::new(operations.clone()).build();

        assert!(document.is_ok());

        let mut exp_result = DocumentViewFields::new();
        exp_result.insert(
            "name",
            DocumentViewValue::new(
                operations[4].operation_id(),
                &OperationValue::Text("Polar Bear Cafe!!!!!!!!!!".to_string()),
            ),
        );

        let expected_graph_tips: Vec<OperationId> =
            vec![operations[4].clone().operation_id().clone()];
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
            &DocumentId::new(operations[0].operation_id().to_owned())
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
            &DocumentId::new(operations[0].operation_id().to_owned())
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
    }

    #[rstest]
    fn must_have_create_operation(#[from(random_key_pair)] key_pair_1: KeyPair) {
        let panda = Client::new("panda".to_string(), key_pair_1);
        let test_schema = schema_item(
            schema(TEST_SCHEMA_ID),
            "",
            vec![("name", FieldType::String)],
        );
        let mut node = Node::new(vec![test_schema.clone()]);

        // Panda publishes a create operation.
        // This instantiates a new document.
        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(&[("name", OperationValue::Text("Panda Cafe".to_string()))]),
        )
        .unwrap();

        // Panda publishes an update operation.
        // It contains the id of the previous operation in it's `previous_operations` array
        send_to_node(
            &mut node,
            &panda,
            &update_operation(
                &[("name", OperationValue::Text("Panda Cafe!".to_string()))],
                &panda_entry_1_hash.into(),
            ),
        )
        .unwrap();

        // Only retrieve the update operation.
        let only_the_update_operation = &node.all_entries()[1];

        let operations = vec![VerifiedOperation::new_from_entry(
            &only_the_update_operation.entry_encoded(),
            &only_the_update_operation.operation_encoded(),
            &test_schema,
        )
        .unwrap()];

        assert_eq!(
            DocumentBuilder::new(operations)
                .build()
                .unwrap_err()
                .to_string(),
            "Every document must contain one create operation".to_string()
        );
    }

    #[rstest]
    fn incorrect_previous_operations(
        #[from(random_key_pair)] key_pair_1: KeyPair,
        #[from(random_document_view_id)] incorrect_previous_operation: DocumentViewId,
    ) {
        let panda = Client::new("panda".to_string(), key_pair_1);
        let test_schema = schema_item(
            schema(TEST_SCHEMA_ID),
            "",
            vec![("name", FieldType::String)],
        );
        let mut node = Node::new(vec![test_schema.clone()]);

        // Panda publishes a create operation.
        // This instantiates a new document.
        let (panda_entry_1_hash, next_entry_args) = send_to_node(
            &mut node,
            &panda,
            &create_operation(&[("name", OperationValue::Text("Panda Cafe".to_string()))]),
        )
        .unwrap();

        // Construct an update operation with non-existant previous operations
        let operation_with_wrong_prev_ops = update_operation(
            &[("name", OperationValue::Text("Panda Cafe!".to_string()))],
            &incorrect_previous_operation,
        );

        let entry_one = node.get_entry(&panda_entry_1_hash);

        let operation_one = VerifiedOperation::new_from_entry(
            &entry_one.entry_encoded(),
            &entry_one.operation_encoded(),
            &test_schema,
        )
        .unwrap();

        let entry_two =
            panda.signed_encoded_entry(operation_with_wrong_prev_ops.clone(), next_entry_args);

        let operation_two = VerifiedOperation::new_from_entry(
            &entry_two,
            &OperationEncoded::try_from(&operation_with_wrong_prev_ops).unwrap(),
            &test_schema,
        )
        .unwrap();

        assert_eq!(
            DocumentBuilder::new(vec![operation_one, operation_two.clone()])
                .build()
                .unwrap_err()
                .to_string(),
            format!(
                "Operation {} cannot be connected to the document graph",
                operation_two.operation_id()
            )
        );
    }

    #[rstest]
    fn operation_schemas_not_matching(#[from(random_key_pair)] key_pair_1: KeyPair) {
        let panda = Client::new("panda".to_string(), key_pair_1);
        let test_schema = schema_item(
            schema(TEST_SCHEMA_ID),
            "",
            vec![("name", FieldType::String)],
        );
        let mut node = Node::new(vec![test_schema.clone()]);

        // Panda publishes a create operation.
        // This instantiates a new document.
        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(&[("name", OperationValue::Text("Panda Cafe".to_string()))]),
        )
        .unwrap();

        // Panda publishes an update operation but with the wrong schema.
        let (_panda_entry_2_hash, _) = send_to_node(
            &mut node,
            &panda,
            &operation(
                Some(operation_fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe!".to_string()),
                )])),
                Some(panda_entry_1_hash.into()),
                Some(SchemaId::new("schema_definition_v1").unwrap()),
            ),
        )
        .unwrap();

        let operations: Vec<VerifiedOperation> = node
            .all_entries()
            .into_iter()
            .map(|entry| {
                VerifiedOperation::new_from_entry(
                    &entry.entry_encoded(),
                    &entry.operation_encoded(),
                    &test_schema,
                )
                .unwrap()
            })
            .collect();

        assert_eq!(
            DocumentBuilder::new(operations)
                .build()
                .unwrap_err()
                .to_string(),
            "All operations in a document must follow the same schema".to_string()
        );
    }

    #[rstest]
    fn is_deleted(#[from(random_key_pair)] key_pair_1: KeyPair) {
        let panda = Client::new("panda".to_string(), key_pair_1);
        let test_schema = schema_item(
            schema(TEST_SCHEMA_ID),
            "",
            vec![("name", FieldType::String)],
        );
        let mut node = Node::new(vec![test_schema.clone()]);

        // Panda publishes a create operation.
        // This instantiates a new document.
        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(&[("name", OperationValue::Text("Panda Cafe".to_string()))]),
        )
        .unwrap();

        // Panda publishes an delete operation.
        // It contains the id of the previous operation in it's `previous_operations` array.
        send_to_node(
            &mut node,
            &panda,
            &delete_operation(&panda_entry_1_hash.into()),
        )
        .unwrap();

        let operations: Vec<VerifiedOperation> = node
            .all_entries()
            .into_iter()
            .map(|entry| {
                VerifiedOperation::new_from_entry(
                    &entry.entry_encoded(),
                    &entry.operation_encoded(),
                    &test_schema,
                )
                .unwrap()
            })
            .collect();

        let document = DocumentBuilder::new(operations).build().unwrap();

        assert!(document.is_deleted());

        assert!(document.view().is_none());
    }

    #[rstest]
    fn more_than_one_create(#[from(random_key_pair)] key_pair_1: KeyPair) {
        let panda = Client::new("panda".to_string(), key_pair_1);
        let test_schema = schema_item(
            schema(TEST_SCHEMA_ID),
            "",
            vec![("name", FieldType::String)],
        );
        let mut node = Node::new(vec![test_schema.clone()]);

        // Panda publishes a create operation.
        // This instantiates a new document.
        let (_panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(&[("name", OperationValue::Text("Panda Cafe".to_string()))]),
        )
        .unwrap();

        let published_create_operation = &node.all_entries()[0];

        let create_verified_operation = VerifiedOperation::new_from_entry(
            &published_create_operation.entry_encoded(),
            &published_create_operation.operation_encoded(),
            &test_schema,
        )
        .unwrap();

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
    }

    #[rstest]
    fn doc_test() {
        let polar = Client::new(
            "polar".to_string(),
            KeyPair::from_private_key_str(
                "ddcafe34db2625af34c8ba3cf35d46e23283d908c9848c8b43d1f5d0fde779ea",
            )
            .unwrap(),
        );
        let panda = Client::new(
            "panda".to_string(),
            KeyPair::from_private_key_str(
                "1d86b2524b48f0ba86103cddc6bdfd87774ab77ab4c0ea989ed0eeab3d28827a",
            )
            .unwrap(),
        );

        let test_schema = schema_item(
            schema(TEST_SCHEMA_ID),
            "",
            vec![
                ("name", FieldType::String),
                ("owner", FieldType::String),
                ("house-number", FieldType::Int),
            ],
        );
        let mut node = Node::new(vec![test_schema.clone()]);
        let (polar_entry_1_hash, _) = send_to_node(
            &mut node,
            &polar,
            &create_operation(&[
                ("name", OperationValue::Text("Polar Bear Cafe".to_string())),
                ("owner", OperationValue::Text("Polar Bear".to_string())),
                ("house-number", OperationValue::Integer(12)),
            ]),
        )
        .unwrap();
        let (polar_entry_2_hash, _) = send_to_node(
            &mut node,
            &polar,
            &update_operation(
                &[
                    ("name", OperationValue::Text("ʕ •ᴥ•ʔ Cafe!".to_string())),
                    ("owner", OperationValue::Text("しろくま".to_string())),
                ],
                &polar_entry_1_hash.clone().into(),
            ),
        )
        .unwrap();
        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &update_operation(
                &[("name", OperationValue::Text("🐼 Cafe!!".to_string()))],
                &polar_entry_1_hash.clone().into(),
            ),
        )
        .unwrap();
        let (polar_entry_3_hash, _) = send_to_node(
            &mut node,
            &polar,
            &update_operation(
                &[("house-number", OperationValue::Integer(102))],
                &DocumentViewId::new(&[
                    panda_entry_1_hash.clone().into(),
                    polar_entry_2_hash.clone().into(),
                ])
                .unwrap(),
            ),
        )
        .unwrap();
        let (polar_entry_4_hash, _) = send_to_node(
            &mut node,
            &polar,
            &delete_operation(&polar_entry_3_hash.clone().into()),
        )
        .unwrap();

        let operations: Vec<VerifiedOperation> = [
            polar_entry_1_hash,
            polar_entry_2_hash,
            panda_entry_1_hash,
            polar_entry_3_hash,
            polar_entry_4_hash,
        ]
        .iter()
        .map(|hash| {
            let entry = node.get_entry(hash);
            VerifiedOperation::new_from_entry(
                &entry.entry_encoded(),
                &entry.operation_encoded(),
                &test_schema,
            )
            .unwrap()
        })
        .collect();

        // These two operations were both published by the same author and they form a simple
        // update graph which looks like this:
        //
        //   ++++++++++++++++++++++++++++    ++++++++++++++++++++++++++++
        //   | name : "Polar Bear Cafe" |    | name : "ʕ •ᴥ•ʔ Cafe!"    |
        //   | owner: "Polar Bear"      |<---| owner: "しろくま"　　　　　 |
        //   | house-number: 12         |    ++++++++++++++++++++++++++++
        //   ++++++++++++++++++++++++++++
        //
        // With these operations we can construct a new document like so:
        let document = DocumentBuilder::new(operations[0..2].to_vec()).build();

        // Which is _Ok_ because the collection of operations are valid (there should be exactly
        // one CREATE operation, they are all causally linked, all operations should follow the
        // same schema).
        assert!(document.is_ok());

        let document = document.unwrap();
        assert_eq!(format!("{}", document), "<Document f21e48>");

        // This process already builds, sorts and reduces the document. We can now
        // access the derived view to check it's values.

        let mut expected_fields = DocumentViewFields::new();
        expected_fields.insert(
            "name",
            DocumentViewValue::new(
                operations[1].operation_id(),
                &OperationValue::Text("ʕ •ᴥ•ʔ Cafe!".into()),
            ),
        );
        expected_fields.insert(
            "owner",
            DocumentViewValue::new(
                operations[1].operation_id(),
                &OperationValue::Text("しろくま".into()),
            ),
        );
        expected_fields.insert(
            "house-number",
            DocumentViewValue::new(operations[0].operation_id(), &OperationValue::Integer(12)),
        );

        let document_view = document.view().unwrap();

        assert_eq!(document_view.fields(), &expected_fields);

        // If another operation arrives, from a different author, which has a causal relation
        // to the original operation, then we have a new branch in the graph, it might look like
        // this:
        //
        //   ++++++++++++++++++++++++++++    +++++++++++++++++++++++++++
        //   | name : "Polar Bear Cafe" |    | name :  "ʕ •ᴥ•ʔ Cafe!"  |
        //   | owner: "Polar Bear"      |<---| owner: "しろくま"　　　　　|
        //   | house-number: 12         |    +++++++++++++++++++++++++++
        //   ++++++++++++++++++++++++++++
        //                A
        //                |
        //                |                  +++++++++++++++++++++++++++
        //                -----------------  | name: "🐼 Cafe!"        |
        //                                   +++++++++++++++++++++++++++
        //
        // This can happen when the document is edited concurrently at different locations, before
        // either author knew of the others update. It's not a problem though, as a document is
        // traversed a deterministic path is selected and so two matching collections of operations
        // will always be sorted into the same order. When there is a conflict (in this case "name"
        // was changed on both replicas) one of them "just wins" in a last-write-wins fashion.

        // We can build the document agan now with these 3 operations:

        let document = DocumentBuilder::new(operations[0..3].to_vec())
            .build()
            .unwrap();
        let document_view = document.view();

        // Here we see that "🐼 Cafe!" won the conflict, meaning it was applied after "ʕ •ᴥ•ʔ Cafe!".
        expected_fields.insert(
            "name",
            DocumentViewValue::new(
                operations[2].operation_id(),
                &OperationValue::Text("🐼 Cafe!!".into()),
            ),
        );

        assert_eq!(document_view.unwrap().fields(), &expected_fields);

        // Now our first author publishes a 4th operation after having seen the full collection
        // of operations. This results in two links to previous operations being formed. Effectively
        // merging the two graph branches into one again. This is important for retaining update
        // context. Without it, we wouldn't know the relation between operations existing on
        // different branches.
        //
        //   ++++++++++++++++++++++++++++    +++++++++++++++++++++++++++
        //   | name : "Polar Bear Cafe" |    | name :  "ʕ •ᴥ•ʔ Cafe!"  |
        //   | owner: "Polar Bear"      |<---| owner: "しろくま"　　　　　|<---\
        //   | house-number: 12         |    +++++++++++++++++++++++++++     \
        //   ++++++++++++++++++++++++++++                                    ++++++++++++++++++++++
        //                A                                                  | house-number: 102  |
        //                |                                                  ++++++++++++++++++++++
        //                |                  +++++++++++++++++++++++++++     /
        //                -----------------  | name: "🐼 Cafe!"        |<---/
        //                                   +++++++++++++++++++++++++++
        //

        let document = DocumentBuilder::new(operations[0..4].to_vec())
            .build()
            .unwrap();

        expected_fields.insert(
            "house-number",
            DocumentViewValue::new(operations[3].operation_id(), &OperationValue::Integer(102)),
        );

        assert_eq!(document.view().unwrap().fields(), &expected_fields);

        // Finally, we want to delete the document, for this we publish a DELETE operation.

        let document = DocumentBuilder::new(operations[0..5].to_vec())
            .build()
            .unwrap();

        assert!(document.view().is_none());
        assert!(document.is_deleted());
    }

    #[rstest]
    fn builds_specific_document_view() {
        let panda = Client::new(
            "panda".to_string(),
            KeyPair::from_private_key_str(
                "ddcafe34db2625af34c8ba3cf35d46e23283d908c9848c8b43d1f5d0fde779ea",
            )
            .unwrap(),
        );

        let test_schema = schema_item(
            schema(TEST_SCHEMA_ID),
            "",
            vec![("name", FieldType::String)],
        );
        let mut node = Node::new(vec![test_schema.clone()]);

        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(&[("name", OperationValue::Text("Panda Cafe".to_string()))]),
        )
        .unwrap();

        let (panda_entry_2_hash, _) = send_to_node(
            &mut node,
            &panda,
            &update_operation(
                &[("name", OperationValue::Text("Panda Cafe!".to_string()))],
                &panda_entry_1_hash.clone().into(),
            ),
        )
        .unwrap();

        let (panda_entry_3_hash, _) = send_to_node(
            &mut node,
            &panda,
            &update_operation(
                &[("name", OperationValue::Text("Panda Cafe!!!!!!".to_string()))],
                &panda_entry_1_hash.clone().into(),
            ),
        )
        .unwrap();

        // DOCUMENT: [panda_1]<--[penguin_1]
        //                    \----[panda_2]

        let operations: Vec<VerifiedOperation> = node
            .all_entries()
            .into_iter()
            .map(|entry| {
                VerifiedOperation::new_from_entry(
                    &entry.entry_encoded(),
                    &entry.operation_encoded(),
                    &test_schema,
                )
                .unwrap()
            })
            .collect();

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
    }
}
