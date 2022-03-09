// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::convert::TryInto;

use crate::document::{DocumentBuilderError, DocumentId, DocumentView, DocumentViewId};
use crate::graph::Graph;
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{AsOperation, OperationValue, OperationWithMeta};
use crate::schema::system::{SchemaFieldView, SchemaView};
use crate::schema::SchemaId;

use super::AsDocumentView;

/// Construct a graph from a list of operations.
pub(super) fn build_graph(
    operations: &[OperationWithMeta],
) -> Result<Graph<OperationWithMeta>, DocumentBuilderError> {
    let mut graph = Graph::new();

    // Add all operations to the graph.
    for operation in operations {
        graph.add_node(operation.operation_id().as_str(), operation.clone());
    }

    // Add links between operations in the graph.
    for operation in operations {
        if let Some(previous_operations) = operation.previous_operations() {
            for previous in previous_operations {
                let success = graph.add_link(previous.as_str(), operation.operation_id().as_str());
                if !success {
                    return Err(DocumentBuilderError::InvalidOperationLink(
                        operation.operation_id().as_str().into(),
                    ));
                }
            }
        }
    }

    Ok(graph)
}

type FieldKey = String;
type IsEdited = bool;
type IsDeleted = bool;

/// Reduce a list of operations into a single view.
pub(super) fn reduce<T: AsOperation>(
    ordered_operations: &[T],
) -> (BTreeMap<FieldKey, OperationValue>, IsEdited, IsDeleted) {
    let is_edited = ordered_operations.len() > 1;
    let mut is_deleted = false;

    let mut view = BTreeMap::new();

    for operation in ordered_operations {
        if operation.is_delete() {
            is_deleted = true
        }

        if let Some(fields) = operation.fields() {
            for (key, value) in fields.iter() {
                view.insert(key.to_string(), value.to_owned());
            }
        }
    }

    (view, is_edited, is_deleted)
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
#[derive(Debug)]
pub struct Document {
    id: DocumentId,
    author: Author,
    schema: SchemaId,
    view: Box<dyn AsDocumentView>,
    meta: DocumentMeta,
}

impl Document {
    /// Get the document id.
    pub fn id(&self) -> &DocumentId {
        &self.id
    }

    /// Get the document view id.
    pub fn view_id(&self) -> &DocumentViewId {
        self.view.id()
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
    pub fn view(&self) -> &Box<dyn AsDocumentView> {
        &self.view
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
        let graph_tips: Vec<Hash> = sorted_graph_data
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
        let document_view_id = DocumentViewId::new(graph_tips);

        // Construct the document view, from the reduced values and the document view id
        let document_view = DocumentView::new(document_view_id, view);

        // Depending on the defined schema, convert into it's associated document view
        let specific_document_view: Box<dyn AsDocumentView> = match schema {
            SchemaId::Application(_) => Box::new(document_view),
            SchemaId::Schema => {
                // Here we can unwrap as we are assuming the operations
                // contained in this view were already validated against
                // the schema_format CDDL definition.
                let view: SchemaView = document_view.try_into().unwrap();
                Box::new(view)
            }
            SchemaId::SchemaField => {
                let view: SchemaFieldView = document_view.try_into().unwrap();
                Box::new(view)
            }
        };

        Ok(Document {
            id: document_id,
            schema,
            author,
            view: specific_document_view,
            meta,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rstest::rstest;

    use crate::document::DocumentId;
    use crate::identity::KeyPair;
    use crate::operation::{Operation, OperationValue, OperationWithMeta};
    use crate::schema::SchemaId;
    use crate::test_utils::constants::DEFAULT_SCHEMA_HASH;
    use crate::test_utils::fixtures::{
        create_operation, delete_operation, fields, random_key_pair, schema, update_operation,
    };
    use crate::test_utils::mocks::{send_to_node, Client, Node};
    use crate::test_utils::utils::operation_fields;

    use super::{reduce, DocumentBuilder};

    #[rstest]
    fn reduces_operations(
        create_operation: Operation,
        update_operation: Operation,
        delete_operation: Operation,
    ) {
        let (reduced_create, is_edited, is_deleted) = reduce(&[create_operation.clone()]);
        assert_eq!(
            reduced_create.get("message").unwrap(),
            &OperationValue::Text("Hello!".to_string())
        );
        assert!(!is_edited);
        assert!(!is_deleted);

        let (reduced_update, is_edited, is_deleted) =
            reduce(&[create_operation.clone(), update_operation.clone()]);
        assert_eq!(
            reduced_update.get("message").unwrap(),
            &OperationValue::Text("Updated, hello!".to_string())
        );
        assert!(is_edited);
        assert!(!is_deleted);

        let (reduced_delete, is_edited, is_deleted) =
            reduce(&[create_operation, update_operation, delete_operation]);
        // The value remains the same, but the deleted flag is true now.
        assert_eq!(
            reduced_delete.get("message").unwrap(),
            &OperationValue::Text("Updated, hello!".to_string())
        );
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
        // It contains the hash of the previous operation in it's `previous_operations` array
        //
        // DOCUMENT: [panda_1]<--[panda_2]
        //
        let (panda_entry_2_hash, _) = send_to_node(
            &mut node,
            &panda,
            &update_operation(
                schema.clone(),
                vec![panda_entry_1_hash.clone()],
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
                vec![panda_entry_1_hash.clone()],
                fields(vec![(
                    "name",
                    OperationValue::Text("Penguin Cafe".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Penguin publishes a new operation while now being aware of the previous branching situation.
        // Their `previous_operations` field now contains 2 operation hash id's.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]<---[penguin_2]
        //                    \----[panda_2]<--/
        let (penguin_entry_2_hash, _) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                schema.clone(),
                vec![penguin_entry_1_hash.clone(), panda_entry_2_hash.clone()],
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
                vec![penguin_entry_2_hash.clone()],
                fields(vec![(
                    "name",
                    OperationValue::Text("Polar Bear Cafe!!!!!!!!!!".to_string()),
                )]),
            ),
        )
        .unwrap();

        let entry_1 = node.get_entry(&panda_entry_1_hash);
        let panda_1 =
            OperationWithMeta::new(&entry_1.entry_encoded(), &entry_1.operation_encoded()).unwrap();
        let entry_2 = node.get_entry(&panda_entry_2_hash);
        let panda_2 =
            OperationWithMeta::new(&entry_2.entry_encoded(), &entry_2.operation_encoded()).unwrap();
        let entry_3 = node.get_entry(&penguin_entry_1_hash);
        let penguin_1 =
            OperationWithMeta::new(&entry_3.entry_encoded(), &entry_3.operation_encoded()).unwrap();
        let entry_4 = node.get_entry(&penguin_entry_2_hash);
        let penguin_2 =
            OperationWithMeta::new(&entry_4.entry_encoded(), &entry_4.operation_encoded()).unwrap();
        let entry_5 = node.get_entry(&penguin_entry_3_hash);
        let penguin_3 =
            OperationWithMeta::new(&entry_5.entry_encoded(), &entry_5.operation_encoded()).unwrap();

        let operations = vec![
            panda_1.clone(),
            panda_2.clone(),
            penguin_1.clone(),
            penguin_2.clone(),
            penguin_3.clone(),
        ];

        let document = DocumentBuilder::new(operations).build();

        assert!(document.is_ok());

        let mut exp_result = BTreeMap::new();
        exp_result.insert(
            "name".to_string(),
            OperationValue::Text("Polar Bear Cafe!!!!!!!!!!".to_string()),
        );
        let expected_graph_tip = vec![penguin_entry_3_hash.clone()];
        let expected_op_order = vec![
            panda_1.clone(),
            panda_2.clone(),
            penguin_1.clone(),
            penguin_2.clone(),
            penguin_3.clone(),
        ];

        // Document should resolve to expected value

        let document = document.unwrap();
        assert_eq!(document.view().get("name"), exp_result.get("name"));
        assert!(document.is_edited());
        assert!(!document.is_deleted());
        assert_eq!(document.operations(), &expected_op_order);
        assert_eq!(document.view_id().graph_tips(), &expected_graph_tip);
        assert_eq!(document.id(), &DocumentId::new(panda_entry_1_hash.clone()));

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

        assert_eq!(replica_1.view().get("name"), replica_2.view().get("name"));
        assert_eq!(replica_1.view().get("name"), replica_3.view().get("name"));
        assert_eq!(replica_1.id(), &DocumentId::new(panda_entry_1_hash.clone()));
        assert_eq!(
            replica_1.view_id().graph_tips(),
            &[penguin_entry_3_hash.clone()]
        );
        assert_eq!(replica_2.id(), &DocumentId::new(panda_entry_1_hash.clone()));
        assert_eq!(
            replica_2.view_id().graph_tips(),
            &[penguin_entry_3_hash.clone()]
        );
        assert_eq!(replica_3.id(), &DocumentId::new(panda_entry_1_hash));
        assert_eq!(replica_3.view_id().graph_tips(), &[penguin_entry_3_hash]);
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
                "1c86b2524b48f0ba86103cddc6bdfd87774ab77ab4c0ea989ed0eeab3d28827a",
            )
            .unwrap(),
        );
        let schema = SchemaId::new(DEFAULT_SCHEMA_HASH).unwrap();
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
                vec![polar_entry_1_hash.clone()],
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
                vec![polar_entry_1_hash.clone()],
                operation_fields(vec![("name", OperationValue::Text("🐼 Cafe!".to_string()))]),
            ),
        )
        .unwrap();
        let (polar_entry_3_hash, _) = send_to_node(
            &mut node,
            &polar,
            &update_operation(
                schema.clone(),
                vec![panda_entry_1_hash.clone(), polar_entry_2_hash.clone()],
                operation_fields(vec![("house-number", OperationValue::Integer(102))]),
            ),
        )
        .unwrap();
        let (polar_entry_4_hash, _) = send_to_node(
            &mut node,
            &polar,
            &delete_operation(schema, vec![polar_entry_3_hash.clone()]),
        )
        .unwrap();
        let entry_1 = node.get_entry(&polar_entry_1_hash);
        let operation_1 =
            OperationWithMeta::new(&entry_1.entry_encoded(), &entry_1.operation_encoded()).unwrap();
        let entry_2 = node.get_entry(&polar_entry_2_hash);
        let operation_2 =
            OperationWithMeta::new(&entry_2.entry_encoded(), &entry_2.operation_encoded()).unwrap();
        let entry_3 = node.get_entry(&panda_entry_1_hash);
        let operation_3 =
            OperationWithMeta::new(&entry_3.entry_encoded(), &entry_3.operation_encoded()).unwrap();
        let entry_4 = node.get_entry(&polar_entry_3_hash);
        let operation_4 =
            OperationWithMeta::new(&entry_4.entry_encoded(), &entry_4.operation_encoded()).unwrap();
        let entry_5 = node.get_entry(&polar_entry_4_hash);
        let operation_5 =
            OperationWithMeta::new(&entry_5.entry_encoded(), &entry_5.operation_encoded()).unwrap();

        // Here we have a collection of 2 operations
        let mut operations = vec![
            // CREATE operation: {name: "Polar Bear Cafe", owner: "Polar Bear", house-number: 12}
            operation_1,
            // UPDATE operation: {name: "ʕ •ᴥ•ʔ Cafe!", owner: "しろくま"}
            operation_2,
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

        // This process already builds, sorts and reduces the document. We can now
        // access the derived view to check it's values.

        let document_view = document.view();

        assert_eq!(
            document_view.get("name").unwrap(),
            &OperationValue::Text("ʕ •ᴥ•ʔ Cafe!".into())
        );
        assert_eq!(
            document_view.get("owner").unwrap(),
            &OperationValue::Text("しろくま".into())
        );
        assert_eq!(
            document_view.get("house-number").unwrap(),
            &OperationValue::Integer(12)
        );

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
        operations.push(operation_3);

        let document = DocumentBuilder::new(operations.clone()).build().unwrap();
        let document_view = document.view();

        // Here we see that "🐼 Cafe!" won the conflict, meaning it was applied after "ʕ •ᴥ•ʔ Cafe!".
        assert_eq!(
            document_view.get("name").unwrap(),
            &OperationValue::Text("🐼 Cafe!".into())
        );
        assert_eq!(
            document_view.get("owner").unwrap(),
            &OperationValue::Text("しろくま".into())
        );
        assert_eq!(
            document_view.get("house-number").unwrap(),
            &OperationValue::Integer(12)
        );

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
        operations.push(operation_4);

        let document = DocumentBuilder::new(operations.clone()).build().unwrap();
        let document_view = document.view();

        assert_eq!(
            document_view.get("name").unwrap(),
            &OperationValue::Text("🐼 Cafe!".into())
        );
        assert_eq!(
            document_view.get("owner").unwrap(),
            &OperationValue::Text("しろくま".into())
        );
        assert_eq!(
            document_view.get("house-number").unwrap(),
            &OperationValue::Integer(102)
        );

        // Finally, we want to delete the document, for this we publish a DELETE operation.

        // DELETE operation: {}
        operations.push(operation_5);

        let document = DocumentBuilder::new(operations.clone()).build().unwrap();
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
        // It contains the hash of the previous operation in it's `previous_operations` array
        send_to_node(
            &mut node,
            &panda,
            &update_operation(
                schema,
                vec![panda_entry_1_hash],
                fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe!".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Only retrieve the update operation.
        let only_the_update_operation = &node.all_entries()[1];

        let operations = vec![OperationWithMeta::new(
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
        // It contains the hash of the previous operation in it's `previous_operations` array.
        send_to_node(
            &mut node,
            &panda,
            &delete_operation(schema, vec![panda_entry_1_hash]),
        )
        .unwrap();

        let operations: Vec<OperationWithMeta> = node
            .all_entries()
            .into_iter()
            .map(|entry| {
                OperationWithMeta::new(&entry.entry_encoded(), &entry.operation_encoded()).unwrap()
            })
            .collect();

        assert!(DocumentBuilder::new(operations)
            .build()
            .unwrap()
            .is_deleted());
    }
}
