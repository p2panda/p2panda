// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;

use crate::document::error::DocumentBuilderError;
use crate::document::materialization::{build_graph, reduce};
use crate::document::{DocumentId, DocumentView, DocumentViewId};
use crate::identity::PublicKey;
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
    author: PublicKey,
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

    /// Get the document author's public key.
    pub fn author(&self) -> &PublicKey {
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
                previous: operation.previous(),
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

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::{DocumentId, DocumentViewFields, DocumentViewId, DocumentViewValue};
    use crate::entry::traits::AsEncodedEntry;
    use crate::identity::KeyPair;
    use crate::operation::traits::AsVerifiedOperation;
    use crate::operation::{
        OperationAction, OperationBuilder, OperationId, OperationValue, VerifiedOperation,
    };
    use crate::schema::{FieldType, Schema, SchemaId};
    use crate::test_utils::constants::{self, PRIVATE_KEY};
    use crate::test_utils::db::test_db::send_to_store;
    use crate::test_utils::db::MemoryStore;
    use crate::test_utils::fixtures::{
        operation_fields, random_document_view_id, schema, verified_operation,
    };
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

    #[rstest]
    #[tokio::test]
    async fn resolve_documents(
        #[with(vec![("name".to_string(), FieldType::String)])] schema: Schema,
    ) {
        let panda = KeyPair::from_private_key_str(
            "ddcafe34db2625af34c8ba3cf35d46e23283d908c9848c8b43d1f5d0fde779ea",
        )
        .unwrap();

        let penguin = KeyPair::from_private_key_str(
            "1c86b2524b48f0ba86103cddc6bdfd87774ab77ab4c0ea989ed0eeab3d28827a",
        )
        .unwrap();

        let store = MemoryStore::default();

        // Panda publishes a CREATE operation.
        // This instantiates a new document.
        //
        // DOCUMENT: [panda_1]

        let panda_operation_1 = OperationBuilder::new(schema.id())
            .action(OperationAction::Create)
            .fields(&[("name", OperationValue::String("Panda Cafe".to_string()))])
            .build()
            .unwrap();

        let (panda_entry_1, _) = send_to_store(&store, &panda_operation_1, &schema, &panda)
            .await
            .unwrap();

        // Panda publishes an UPDATE operation.
        // It contains the id of the previous operation in it's `previous` array
        //
        // DOCUMENT: [panda_1]<--[panda_2]
        //

        let panda_operation_2 = OperationBuilder::new(schema.id())
            .action(OperationAction::Update)
            .fields(&[("name", OperationValue::String("Panda Cafe!".to_string()))])
            .previous(&panda_entry_1.hash().into())
            .build()
            .unwrap();

        let (panda_entry_2, _) = send_to_store(&store, &panda_operation_2, &schema, &panda)
            .await
            .unwrap();

        // Penguin publishes an update operation which creates a new branch in the graph.
        // This is because they didn't know about Panda's second operation.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]
        //                    \----[panda_2]

        let penguin_operation_1 = OperationBuilder::new(schema.id())
            .action(OperationAction::Update)
            .fields(&[(
                "name",
                OperationValue::String("Penguin Cafe!!!".to_string()),
            )])
            .previous(&panda_entry_1.hash().into())
            .build()
            .unwrap();

        let (penguin_entry_1, _) = send_to_store(&store, &penguin_operation_1, &schema, &penguin)
            .await
            .unwrap();

        // Penguin publishes a new operation while now being aware of the previous branching situation.
        // Their `previous` field now contains 2 operation id's.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]<---[penguin_2]
        //                    \----[panda_2]<--/

        let penguin_operation_2 = OperationBuilder::new(schema.id())
            .action(OperationAction::Update)
            .fields(&[(
                "name",
                OperationValue::String("Polar Bear Cafe".to_string()),
            )])
            .previous(&DocumentViewId::new(&[
                penguin_entry_1.hash().into(),
                panda_entry_2.hash().into(),
            ]))
            .build()
            .unwrap();

        let (penguin_entry_2, _) = send_to_store(&store, &penguin_operation_2, &schema, &penguin)
            .await
            .unwrap();

        // Penguin publishes a new update operation which points at the current graph tip.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]<---[penguin_2]<--[penguin_3]
        //                    \----[panda_2]<--/

        let penguin_operation_3 = OperationBuilder::new(schema.id())
            .action(OperationAction::Update)
            .fields(&[(
                "name",
                OperationValue::String("Polar Bear Cafe!!!!!!!!!!".to_string()),
            )])
            .previous(&penguin_entry_2.hash().into())
            .build()
            .unwrap();

        let (penguin_entry_3, _) = send_to_store(&store, &penguin_operation_3, &schema, &penguin)
            .await
            .unwrap();

        let operations: Vec<VerifiedOperation> = store
            .operations
            .lock()
            .unwrap()
            .values()
            .map(|(_, operation)| operation.to_owned())
            .collect();

        let document = DocumentBuilder::new(operations.clone()).build();

        assert!(document.is_ok());

        // Document should resolve to expected value
        let document = document.unwrap();

        let operation_order: Vec<OperationId> = document
            .operations()
            .iter()
            .map(|op| op.id().to_owned())
            .collect();

        let mut exp_result = DocumentViewFields::new();
        exp_result.insert(
            "name",
            DocumentViewValue::new(
                &penguin_entry_3.hash().into(),
                &OperationValue::String("Polar Bear Cafe!!!!!!!!!!".to_string()),
            ),
        );

        let document_id = DocumentId::new(&panda_entry_1.hash().into());
        let expected_graph_tips: Vec<OperationId> = vec![penguin_entry_3.hash().into()];
        let expected_op_order: Vec<OperationId> = vec![
            panda_entry_1.clone(),
            panda_entry_2,
            penguin_entry_1,
            penguin_entry_2,
            penguin_entry_3,
        ]
        .iter()
        .map(|entry| entry.hash().into())
        .collect();

        assert_eq!(document.view().unwrap().get("name"), exp_result.get("name"));
        assert!(document.is_edited());
        assert!(!document.is_deleted());
        assert_eq!(document.author(), &panda.public_key());
        assert_eq!(document.schema(), schema.id());
        assert_eq!(operation_order, expected_op_order);
        assert_eq!(document.view_id().graph_tips(), expected_graph_tips);
        assert_eq!(document.id(), &document_id);

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
        assert_eq!(replica_1.author(), &panda.public_key());
        assert_eq!(replica_1.schema(), schema.id());
        assert_eq!(operation_order, expected_op_order);
        assert_eq!(replica_1.view_id().graph_tips(), expected_graph_tips);
        assert_eq!(replica_1.id(), &document_id);

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
    fn must_have_create_operation(
        #[from(verified_operation)]
        #[with(
            Some(operation_fields(constants::test_fields())),
            constants::schema(),
            Some(random_document_view_id())
        )]
        update_operation: VerifiedOperation,
    ) {
        assert_eq!(
            DocumentBuilder::new(vec![update_operation])
                .build()
                .unwrap_err()
                .to_string(),
            "every document must contain one create operation".to_string()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn incorrect_previous_operations(
        #[from(verified_operation)]
        #[with(Some(operation_fields(constants::test_fields())), constants::schema())]
        create_operation: VerifiedOperation,
        #[from(verified_operation)]
        #[with(
            Some(operation_fields(constants::test_fields())),
            constants::schema(),
            Some(random_document_view_id())
        )]
        update_operation: VerifiedOperation,
    ) {
        assert_eq!(
            DocumentBuilder::new(vec![create_operation, update_operation.clone()])
                .build()
                .unwrap_err()
                .to_string(),
            format!(
                "operation {} cannot be connected to the document graph",
                update_operation.id()
            )
        );
    }

    #[rstest]
    #[tokio::test]
    async fn operation_schemas_not_matching() {
        let create_operation = verified_operation(
            Some(operation_fields(constants::test_fields())),
            constants::schema(),
            None,
            KeyPair::from_private_key_str(PRIVATE_KEY).unwrap(),
        );

        let update_operation = verified_operation(
            Some(operation_fields(vec![
                ("name", "is_cute".into()),
                ("type", "bool".into()),
            ])),
            Schema::get_system(SchemaId::SchemaFieldDefinition(1))
                .unwrap()
                .to_owned(),
            Some(create_operation.id().to_owned().into()),
            KeyPair::from_private_key_str(PRIVATE_KEY).unwrap(),
        );

        assert_eq!(
            DocumentBuilder::new(vec![create_operation, update_operation])
                .build()
                .unwrap_err()
                .to_string(),
            "all operations in a document must follow the same schema".to_string()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn is_deleted(
        #[from(verified_operation)]
        #[with(Some(operation_fields(constants::test_fields())), constants::schema())]
        create_operation: VerifiedOperation,
    ) {
        let delete_operation = verified_operation(
            None,
            constants::schema(),
            Some(DocumentViewId::new(&[create_operation.id().to_owned()])),
            KeyPair::from_private_key_str(PRIVATE_KEY).unwrap(),
        );

        let document = DocumentBuilder::new(vec![create_operation, delete_operation])
            .build()
            .unwrap();

        assert!(document.is_deleted());
        assert!(document.view().is_none());
    }

    #[rstest]
    #[tokio::test]
    async fn more_than_one_create(#[from(verified_operation)] create_operation: VerifiedOperation) {
        assert_eq!(
            DocumentBuilder::new(vec![create_operation.clone(), create_operation])
                .build()
                .unwrap_err()
                .to_string(),
            "multiple CREATE operations found".to_string()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn builds_specific_document_view(
        #[with(vec![("name".to_string(), FieldType::String)])] schema: Schema,
    ) {
        let panda = KeyPair::new().public_key().to_owned();
        let penguin = KeyPair::new().public_key().to_owned();

        // Panda publishes a CREATE operation.
        // This instantiates a new document.
        //
        // DOCUMENT: [panda_1]

        let panda_operation_1 = OperationBuilder::new(schema.id())
            .action(OperationAction::Create)
            .fields(&[("name", OperationValue::String("Panda Cafe".to_string()))])
            .build()
            .unwrap();

        let panda_operation_1 = VerifiedOperation::new(
            &panda,
            &panda_operation_1,
            &"0020aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .parse()
                .unwrap(),
        );

        // Panda publishes an UPDATE operation.
        // It contains the id of the previous operation in it's `previous` array
        //
        // DOCUMENT: [panda_1]<--[panda_2]
        //

        let panda_operation_2 = OperationBuilder::new(schema.id())
            .action(OperationAction::Update)
            .fields(&[("name", OperationValue::String("Panda Cafe!".to_string()))])
            .previous(&DocumentViewId::new(&[panda_operation_1.id().to_owned()]))
            .build()
            .unwrap();

        let panda_operation_2 = VerifiedOperation::new(
            &panda,
            &panda_operation_2,
            &"0020bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                .parse()
                .unwrap(),
        );

        // Penguin publishes an update operation which creates a new branch in the graph.
        // This is because they didn't know about Panda's second operation.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]
        //                    \----[panda_2]

        let penguin_operation_1 = OperationBuilder::new(schema.id())
            .action(OperationAction::Update)
            .fields(&[(
                "name",
                OperationValue::String("Penguin Cafe!!!".to_string()),
            )])
            .previous(&DocumentViewId::new(&[panda_operation_2.id().to_owned()]))
            .build()
            .unwrap();

        let penguin_operation_1 = VerifiedOperation::new(
            &penguin,
            &penguin_operation_1,
            &"0020cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                .parse()
                .unwrap(),
        );

        let operations = vec![
            panda_operation_1.clone(),
            panda_operation_2.clone(),
            penguin_operation_1.clone(),
        ];
        let document_builder = DocumentBuilder::new(operations);

        assert_eq!(
            document_builder
                .build_to_view_id(Some(DocumentViewId::new(&[panda_operation_1
                    .id()
                    .to_owned()])))
                .unwrap()
                .view()
                .unwrap()
                .get("name")
                .unwrap()
                .value(),
            &OperationValue::String("Panda Cafe".to_string())
        );

        assert_eq!(
            document_builder
                .build_to_view_id(Some(DocumentViewId::new(&[panda_operation_2
                    .id()
                    .to_owned()])))
                .unwrap()
                .view()
                .unwrap()
                .get("name")
                .unwrap()
                .value(),
            &OperationValue::String("Panda Cafe!".to_string())
        );

        assert_eq!(
            document_builder
                .build_to_view_id(Some(DocumentViewId::new(&[penguin_operation_1
                    .id()
                    .to_owned()])))
                .unwrap()
                .view()
                .unwrap()
                .get("name")
                .unwrap()
                .value(),
            &OperationValue::String("Penguin Cafe!!!".to_string())
        );

        assert_eq!(
            document_builder
                .build_to_view_id(Some(DocumentViewId::new(&[
                    panda_operation_2.id().to_owned(),
                    penguin_operation_1.id().to_owned()
                ])))
                .unwrap()
                .view()
                .unwrap()
                .get("name")
                .unwrap()
                .value(),
            &OperationValue::String("Penguin Cafe!!!".to_string())
        );
    }
}
