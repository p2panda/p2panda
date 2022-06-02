// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;

use crate::document::{
    DocumentBuilderError, DocumentId, DocumentView, DocumentViewFields, DocumentViewId,
    DocumentViewValue,
};
use crate::graph::Graph;
use crate::identity::Author;
use crate::operation::{AsOperation, OperationId, OperationWithMeta};
use crate::schema::SchemaId;

/// Construct a graph from a list of operations.
pub(super) fn build_graph(
    operations: &[OperationWithMeta],
) -> Result<Graph<OperationId, OperationWithMeta>, DocumentBuilderError> {
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
                        operation.operation_id().as_hash().as_str().into(),
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
pub(super) fn reduce(
    ordered_operations: &[OperationWithMeta],
) -> (Option<DocumentViewFields>, IsEdited, IsDeleted) {
    let is_edited = ordered_operations.len() > 1;

    let mut document_view_fields = DocumentViewFields::new();

    for operation in ordered_operations {
        if operation.is_delete() {
            return (None, true, true);
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
    deleted: bool,
    edited: bool,
    operations: Vec<OperationWithMeta>,
}

/// A replicatable data type designed to handle concurrent updates in a way where all replicas
/// eventually resolve to the same deterministic value.
///
/// `Document`s are immutable and contain a resolved document view as well as metadata relating
/// to the specific document instance. These can be accessed through getter methods. To create
/// documents you should use `DocumentBuilder`.
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
    pub fn operations(&self) -> &Vec<OperationWithMeta> {
        &self.meta.operations
    }

    /// Returns true if this document has applied an UPDATE operation.
    pub fn is_edited(&self) -> bool {
        self.meta.edited
    }

    /// Returns true if this document has processed a DELETE operation.
    pub fn is_deleted(&self) -> bool {
        self.meta.deleted
    }
}

impl Display for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<Document {}>", self.id)
    }
}

/// A struct for building [documents][`Document`] from a collection of [operations with
/// metadata][`crate::operation::OperationWithMeta`].
///
/// ## Example
///
/// ```
/// # extern crate p2panda_rs;
/// # #[cfg(test)]
/// # mod tests {
/// # use rstest::rstest;
/// # use p2panda_rs::document::DocumentBuilder;
/// # use p2panda_rs::operation::OperationWithMeta;
/// # use p2panda_rs::test_utils::meta_operation;
/// #
/// # #[rstest]
/// # fn main(#[from(meta_operation)] operation: OperationWithMeta) -> () {
/// // You need a `Vec<OperationWithMeta>` that includes the `CREATE` operation
/// let operations: Vec<OperationWithMeta> = vec![operation];
///
/// // Then you can make a `Document` from it
/// let document = DocumentBuilder::new(operations).build();
/// assert!(document.is_ok());
/// # }
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct DocumentBuilder {
    operations: Vec<OperationWithMeta>,
}

impl DocumentBuilder {
    /// Instantiate a new `DocumentBuilder` from a collection of operations.
    pub fn new(operations: Vec<OperationWithMeta>) -> DocumentBuilder {
        Self { operations }
    }

    /// Get all operations for this document.
    pub fn operations(&self) -> Vec<OperationWithMeta> {
        self.operations.clone()
    }

    /// Validates the set of operations and builds the document.
    ///
    /// The returned document also contains the latest resolved [document view][`DocumentView`].
    ///
    /// Validation checks the following:
    /// - There is exactly one `CREATE` operation.
    /// - All operations are causally connected to the root operation.
    /// - All operations follow the same schema.
    /// - No cycles exist in the graph.
    pub fn build(&self) -> Result<Document, DocumentBuilderError> {
        // Find CREATE operation
        let mut collect_create_operation: Vec<OperationWithMeta> = self
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

        // Build the graph  and then sort the operations into a linear order
        let graph = build_graph(&self.operations)?;
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

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::document_view_fields::{DocumentViewFields, DocumentViewValue};
    use crate::document::DocumentId;
    use crate::identity::KeyPair;
    use crate::operation::{OperationId, OperationValue, OperationWithMeta};
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::{
        create_operation, create_operation_with_meta, delete_operation, delete_operation_with_meta,
        fields, random_key_pair, schema, update_operation, update_operation_with_meta,
    };
    use crate::test_utils::mocks::{send_to_node, Client, Node};
    use crate::test_utils::utils::operation_fields;

    use super::{reduce, DocumentBuilder};

    #[rstest]
    fn reduces_operations(
        #[from(create_operation_with_meta)] create_operation: OperationWithMeta,
        #[from(update_operation_with_meta)] update_operation: OperationWithMeta,
        #[from(delete_operation_with_meta)] delete_operation: OperationWithMeta,
    ) {
        let (reduced_create, is_edited, is_deleted) = reduce(&[create_operation.clone()]);
        assert_eq!(
            *reduced_create.unwrap().get("message").unwrap(),
            DocumentViewValue::new(
                create_operation.operation_id(),
                &OperationValue::Text("Hello!".to_string())
            )
        );
        assert!(!is_edited);
        assert!(!is_deleted);

        let (reduced_update, is_edited, is_deleted) =
            reduce(&[create_operation.clone(), update_operation.clone()]);
        assert_eq!(
            *reduced_update.unwrap().get("message").unwrap(),
            DocumentViewValue::new(
                update_operation.operation_id(),
                &OperationValue::Text("Updated, hello!".to_string())
            )
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
        let mut node = Node::new();

        // Panda publishes a create operation.
        // This instantiates a new document.
        //
        // DOCUMENT: [panda_1]
        //
        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(
                schema.clone(),
                fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe".to_string()),
                )]),
            ),
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
                schema.clone(),
                vec![panda_entry_1_hash.clone().into()],
                fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe!".to_string()),
                )]),
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
                schema.clone(),
                vec![panda_entry_1_hash.clone().into()],
                fields(vec![(
                    "name",
                    OperationValue::Text("Penguin Cafe!!".to_string()),
                )]),
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
                schema.clone(),
                vec![
                    penguin_entry_1_hash.clone().into(),
                    panda_entry_2_hash.clone().into(),
                ],
                fields(vec![(
                    "name",
                    OperationValue::Text("Polar Bear Cafe".to_string()),
                )]),
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
                schema,
                vec![penguin_entry_2_hash.clone().into()],
                fields(vec![(
                    "name",
                    OperationValue::Text("Polar Bear Cafe!!!!!!!!!!".to_string()),
                )]),
            ),
        )
        .unwrap();

        let entry_1 = node.get_entry(&panda_entry_1_hash);
        let panda_1 = OperationWithMeta::new_from_entry(
            &entry_1.entry_encoded(),
            &entry_1.operation_encoded(),
        )
        .unwrap();
        let entry_2 = node.get_entry(&panda_entry_2_hash);
        let panda_2 = OperationWithMeta::new_from_entry(
            &entry_2.entry_encoded(),
            &entry_2.operation_encoded(),
        )
        .unwrap();
        let entry_3 = node.get_entry(&penguin_entry_1_hash);
        let penguin_1 = OperationWithMeta::new_from_entry(
            &entry_3.entry_encoded(),
            &entry_3.operation_encoded(),
        )
        .unwrap();
        let entry_4 = node.get_entry(&penguin_entry_2_hash);
        let penguin_2 = OperationWithMeta::new_from_entry(
            &entry_4.entry_encoded(),
            &entry_4.operation_encoded(),
        )
        .unwrap();
        let entry_5 = node.get_entry(&penguin_entry_3_hash);
        let penguin_3 = OperationWithMeta::new_from_entry(
            &entry_5.entry_encoded(),
            &entry_5.operation_encoded(),
        )
        .unwrap();

        let operations = vec![
            panda_1.clone(),
            panda_2.clone(),
            penguin_1.clone(),
            penguin_2.clone(),
            penguin_3.clone(),
        ];

        let document = DocumentBuilder::new(operations).build();

        assert!(document.is_ok());

        let mut exp_result = DocumentViewFields::new();
        exp_result.insert(
            "name",
            DocumentViewValue::new(
                penguin_3.operation_id(),
                &OperationValue::Text("Polar Bear Cafe!!!!!!!!!!".to_string()),
            ),
        );
        let expected_graph_tips: Vec<OperationId> = vec![penguin_entry_3_hash.clone().into()];
        let expected_op_order = vec![
            panda_1.clone(),
            penguin_1.clone(),
            panda_2.clone(),
            penguin_2.clone(),
            penguin_3.clone(),
        ];

        // Document should resolve to expected value

        let document = document.unwrap();
        assert_eq!(document.view().unwrap().get("name"), exp_result.get("name"));
        assert!(document.is_edited());
        assert!(!document.is_deleted());
        assert_eq!(document.operations(), &expected_op_order);
        assert_eq!(document.view_id().graph_tips(), expected_graph_tips);
        assert_eq!(
            document.id(),
            &DocumentId::new(panda_entry_1_hash.clone().into())
        );

        // Multiple replicas receiving operations in different orders should resolve to same value.

        let replica_1 = DocumentBuilder::new(vec![
            penguin_2.clone(),
            penguin_1.clone(),
            penguin_3.clone(),
            panda_2.clone(),
            panda_1.clone(),
        ])
        .build()
        .unwrap();

        let replica_2 = DocumentBuilder::new(vec![
            penguin_3.clone(),
            panda_2.clone(),
            panda_1.clone(),
            penguin_2.clone(),
            penguin_1.clone(),
        ])
        .build()
        .unwrap();

        let replica_3 =
            DocumentBuilder::new(vec![panda_2, panda_1, penguin_1, penguin_3, penguin_2])
                .build()
                .unwrap();

        assert_eq!(
            replica_1.view().unwrap().get("name"),
            replica_2.view().unwrap().get("name")
        );
        assert_eq!(
            replica_1.view().unwrap().get("name"),
            replica_3.view().unwrap().get("name")
        );
        assert_eq!(
            replica_1.id(),
            &DocumentId::new(panda_entry_1_hash.clone().into())
        );
        assert_eq!(
            replica_1.view_id().graph_tips(),
            &[penguin_entry_3_hash.clone().into()]
        );
        assert_eq!(
            replica_2.id(),
            &DocumentId::from(panda_entry_1_hash.clone())
        );
        assert_eq!(
            replica_2.view_id().graph_tips(),
            &[penguin_entry_3_hash.clone().into()]
        );
        assert_eq!(replica_3.id(), &DocumentId::from(panda_entry_1_hash));
        assert_eq!(
            replica_3.view_id().graph_tips(),
            &[penguin_entry_3_hash.into()]
        );
    }

    #[rstest]
    fn doc_test(schema: SchemaId) {
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

        let mut node = Node::new();
        let (polar_entry_1_hash, _) = send_to_node(
            &mut node,
            &polar,
            &create_operation(
                schema.clone(),
                operation_fields(vec![
                    ("name", OperationValue::Text("Polar Bear Cafe".to_string())),
                    ("owner", OperationValue::Text("Polar Bear".to_string())),
                    ("house-number", OperationValue::Integer(12)),
                ]),
            ),
        )
        .unwrap();
        let (polar_entry_2_hash, _) = send_to_node(
            &mut node,
            &polar,
            &update_operation(
                schema.clone(),
                vec![polar_entry_1_hash.clone().into()],
                operation_fields(vec![
                    ("name", OperationValue::Text("ʕ •ᴥ•ʔ Cafe!".to_string())),
                    ("owner", OperationValue::Text("しろくま".to_string())),
                ]),
            ),
        )
        .unwrap();
        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &update_operation(
                schema.clone(),
                vec![polar_entry_1_hash.clone().into()],
                operation_fields(vec![(
                    "name",
                    OperationValue::Text("🐼 Cafe!!".to_string()),
                )]),
            ),
        )
        .unwrap();
        let (polar_entry_3_hash, _) = send_to_node(
            &mut node,
            &polar,
            &update_operation(
                schema.clone(),
                vec![
                    panda_entry_1_hash.clone().into(),
                    polar_entry_2_hash.clone().into(),
                ],
                operation_fields(vec![("house-number", OperationValue::Integer(102))]),
            ),
        )
        .unwrap();
        let (polar_entry_4_hash, _) = send_to_node(
            &mut node,
            &polar,
            &delete_operation(schema, vec![polar_entry_3_hash.clone().into()]),
        )
        .unwrap();
        let entry_1 = node.get_entry(&polar_entry_1_hash);
        let operation_1 = OperationWithMeta::new_from_entry(
            &entry_1.entry_encoded(),
            &entry_1.operation_encoded(),
        )
        .unwrap();
        let entry_2 = node.get_entry(&polar_entry_2_hash);
        let operation_2 = OperationWithMeta::new_from_entry(
            &entry_2.entry_encoded(),
            &entry_2.operation_encoded(),
        )
        .unwrap();
        let entry_3 = node.get_entry(&panda_entry_1_hash);
        let operation_3 = OperationWithMeta::new_from_entry(
            &entry_3.entry_encoded(),
            &entry_3.operation_encoded(),
        )
        .unwrap();
        let entry_4 = node.get_entry(&polar_entry_3_hash);
        let operation_4 = OperationWithMeta::new_from_entry(
            &entry_4.entry_encoded(),
            &entry_4.operation_encoded(),
        )
        .unwrap();
        let entry_5 = node.get_entry(&polar_entry_4_hash);
        let operation_5 = OperationWithMeta::new_from_entry(
            &entry_5.entry_encoded(),
            &entry_5.operation_encoded(),
        )
        .unwrap();

        // Here we have a collection of 2 operations
        let mut operations = vec![
            // CREATE operation: {name: "Polar Bear Cafe", owner: "Polar Bear", house-number: 12}
            operation_1.clone(),
            // UPDATE operation: {name: "ʕ •ᴥ•ʔ Cafe!", owner: "しろくま"}
            operation_2.clone(),
        ];

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
        let document = DocumentBuilder::new(operations.clone()).build();

        // Which is _Ok_ because the collection of operations are valid (there should be exactly
        // one CREATE operation, they are all causally linked, all operations should follow the
        // same schema).
        assert!(document.is_ok());

        let document = document.unwrap();
        assert_eq!(format!("{}", document), "<Document 52cc67>");

        // This process already builds, sorts and reduces the document. We can now
        // access the derived view to check it's values.

        let mut expected_fields = DocumentViewFields::new();
        expected_fields.insert(
            "name",
            DocumentViewValue::new(
                operation_2.operation_id(),
                &OperationValue::Text("ʕ •ᴥ•ʔ Cafe!".into()),
            ),
        );
        expected_fields.insert(
            "owner",
            DocumentViewValue::new(
                operation_2.operation_id(),
                &OperationValue::Text("しろくま".into()),
            ),
        );
        expected_fields.insert(
            "house-number",
            DocumentViewValue::new(operation_1.operation_id(), &OperationValue::Integer(12)),
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
        //
        // UPDATE operation: {name: "🐼 Cafe!"}
        operations.push(operation_3.clone());

        let document = DocumentBuilder::new(operations.clone()).build().unwrap();
        let document_view = document.view();

        // Here we see that "🐼 Cafe!" won the conflict, meaning it was applied after "ʕ •ᴥ•ʔ Cafe!".
        expected_fields.insert(
            "name",
            DocumentViewValue::new(
                operation_3.operation_id(),
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

        // UPDATE operation: { house-number: 102 }
        operations.push(operation_4.clone());

        let document = DocumentBuilder::new(operations.clone()).build().unwrap();

        expected_fields.insert(
            "house-number",
            DocumentViewValue::new(operation_4.operation_id(), &OperationValue::Integer(102)),
        );

        assert_eq!(document.view().unwrap().fields(), &expected_fields);

        // Finally, we want to delete the document, for this we publish a DELETE operation.

        // DELETE operation: {}
        operations.push(operation_5);

        let document = DocumentBuilder::new(operations.clone()).build().unwrap();

        // expected_fields.insert(
        //     "name",
        //     DocumentViewValue::Deleted(operation_5.operation_id().to_owned()),
        // );
        // expected_fields.insert(
        //     "owner",
        //     DocumentViewValue::Deleted(operation_5.operation_id().to_owned()),
        // );
        // expected_fields.insert(
        //     "house-number",
        //     DocumentViewValue::Deleted(operation_5.operation_id().to_owned()),
        // );

        assert!(document.view().is_none());
        assert!(document.is_deleted());
    }

    #[rstest]
    fn must_have_create_operation(schema: SchemaId, #[from(random_key_pair)] key_pair_1: KeyPair) {
        let panda = Client::new("panda".to_string(), key_pair_1);
        let mut node = Node::new();

        // Panda publishes a create operation.
        // This instantiates a new document.
        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(
                schema.clone(),
                fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Panda publishes an update operation.
        // It contains the id of the previous operation in it's `previous_operations` array
        send_to_node(
            &mut node,
            &panda,
            &update_operation(
                schema,
                vec![panda_entry_1_hash.into()],
                fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe!".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Only retrieve the update operation.
        let only_the_update_operation = &node.all_entries()[1];

        let operations = vec![OperationWithMeta::new_from_entry(
            &only_the_update_operation.entry_encoded(),
            &only_the_update_operation.operation_encoded(),
        )
        .unwrap()];

        assert!(DocumentBuilder::new(operations).build().is_err());
    }

    #[rstest]
    fn is_deleted(schema: SchemaId, #[from(random_key_pair)] key_pair_1: KeyPair) {
        let panda = Client::new("panda".to_string(), key_pair_1);
        let mut node = Node::new();

        // Panda publishes a create operation.
        // This instantiates a new document.
        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(
                schema.clone(),
                fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Panda publishes an delete operation.
        // It contains the id of the previous operation in it's `previous_operations` array.
        send_to_node(
            &mut node,
            &panda,
            &delete_operation(schema, vec![panda_entry_1_hash.into()]),
        )
        .unwrap();

        let operations: Vec<OperationWithMeta> = node
            .all_entries()
            .into_iter()
            .map(|entry| {
                OperationWithMeta::new_from_entry(
                    &entry.entry_encoded(),
                    &entry.operation_encoded(),
                )
                .unwrap()
            })
            .collect();

        let document = DocumentBuilder::new(operations).build().unwrap();

        assert!(document.is_deleted());

        assert!(document.view().is_none());
    }
}
