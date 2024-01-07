// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::{Debug, Display};

use crate::document::error::{DocumentBuilderError, DocumentReducerError};
use crate::document::traits::AsDocument;
use crate::document::{DocumentId, DocumentViewFields, DocumentViewId};
use crate::graph::{Graph, Reducer};
use crate::identity::PublicKey;
use crate::operation::body::traits::Schematic;
use crate::operation::traits::{Actionable, Authored, Fielded, Identifiable};
use crate::operation::OperationId;
use crate::schema::SchemaId;
use crate::Human;

use super::error::DocumentError;

/// High-level datatype representing data published to the p2panda network as key-value pairs.
///
/// Documents are multi-writer and have automatic conflict resolution strategies which produce deterministic
/// state for any two replicas. The underlying structure which make this possible is a directed acyclic graph
/// of [`Operation`]'s. To arrive at the current state of a document the graph is topologically sorted,
/// with any branches being ordered according to the conflicting operations [`OperationId`]. Each operation's
/// mutation is applied in order which results in a LWW (last write wins) resolution strategy.
///
/// All documents have an accompanying `Schema` which describes the shape of the data they will contain. Every
/// operation should have been validated against this schema before being included in the graph.
///
/// Documents are constructed through the [`DocumentBuilder`] or by conversion from vectors of a type implementing
/// the [`Actionable`], [`WithId<OperationId>`] and [`WithPublicKey`].
///
/// To efficiently commit more operations to an already constructed document use the `commit`
/// method. Any operations committed in this way must refer to the documents current view id in
/// their `previous` field.
///
/// See module docs for example uses.
#[derive(Debug, Clone)]
pub struct Document {
    /// The id for this document.
    id: DocumentId,

    /// The data this document contains as key-value pairs.
    fields: Option<DocumentViewFields>,

    /// The id of the schema this document follows.
    schema_id: SchemaId,

    /// The id of the current view of this document.
    view_id: DocumentViewId,

    /// The public key of the author who created this document.
    author: PublicKey,
}

impl AsDocument for Document {
    /// Get the document id.
    fn id(&self) -> &DocumentId {
        &self.id
    }

    /// Get the document view id.
    fn view_id(&self) -> &DocumentViewId {
        &self.view_id
    }

    /// Get the document author's public key.
    fn author(&self) -> &PublicKey {
        &self.author
    }

    /// Get the document schema.
    fn schema_id(&self) -> &SchemaId {
        &self.schema_id
    }

    /// Get the fields of this document.
    fn fields(&self) -> Option<&DocumentViewFields> {
        self.fields.as_ref()
    }

    /// Update the current view of this document.
    fn update_view(&mut self, id: &DocumentViewId, view: Option<&DocumentViewFields>) {
        self.view_id = id.to_owned();
        self.fields = view.cloned();
    }
}

impl Display for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl Human for Document {
    fn display(&self) -> String {
        let offset = blake3::KEY_LEN * 2 - 6;
        format!("<Document {}>", &self.id.as_str()[offset..])
    }
}

/// Struct which implements a Reducer used during document building.
#[derive(Debug, Default)]
struct DocumentReducer {
    document: Option<Document>,
}

/// Implementation of the `Reduce` trait for collections of authored operations.
impl<T> Reducer<T> for DocumentReducer
where
    T: Actionable + Fielded + Identifiable + Schematic + Authored,
{
    type Error = DocumentReducerError;

    /// Combine a visited operation with the existing document.
    fn combine(&mut self, operation: &T) -> Result<(), Self::Error> {
        // Get the current document.
        let document = self.document.clone();

        match document {
            // If it has already been instantiated perform the commit.
            Some(mut document) => {
                match document.commit(operation) {
                    Ok(_) => Ok(()),
                    Err(err) => match err {
                        DocumentError::PreviousDoesNotMatch(_) => {
                            // We accept this error as we are reducing the document while walking
                            // the operation graph in DocumentBuilder. In this situation the
                            // operations are being visited in their topologically sorted order
                            // and in the case of branches, `previous` may not match the documents
                            // current document view id.
                            //
                            // Perform the commit in any case.
                            document.commit_unchecked(operation);
                            Ok(())
                        }
                        // These errors are serious and we should signal that the reducing failed
                        // by storing the error.
                        err => Err(err),
                    },
                }?;
                // Set the updated document.
                self.document = Some(document);
                Ok(())
            }
            // If the document wasn't instantiated yet, then do so.
            None => {
                // Error if this operation is _not_ a CREATE operation.
                if !operation.is_create() {
                    return Err(DocumentReducerError::FirstOperationNotCreate);
                }

                // Construct the document view fields.
                let document_fields = DocumentViewFields::new_from_operation_fields(
                    &operation.id(),
                    &operation.fields().unwrap(),
                );

                // Construct the document.
                let document = Document {
                    id: DocumentId::new(&operation.id()),
                    fields: Some(document_fields),
                    schema_id: operation.schema_id().to_owned(),
                    view_id: DocumentViewId::new(&[operation.id().to_owned()]),
                    author: operation.public_key().to_owned(),
                };

                // Set the newly instantiated document.
                self.document = Some(document);
                Ok(())
            }
        }
    }
}

/// A struct for building [documents][`Document`] from a collection of operations.
#[derive(Debug, Clone)]
pub struct DocumentBuilder<T>(Vec<T>);

impl<T> DocumentBuilder<T>
where
    T: Actionable + Fielded + Identifiable + Schematic + Authored + Debug + Clone + PartialEq,
{
    /// Instantiate a new `DocumentBuilder` from a collection of operations.
    pub fn new(operations: Vec<T>) -> Self {
        Self(operations)
    }

    /// Get all unsorted operations for this document.
    pub fn operations(&self) -> &Vec<T> {
        &self.0
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
    pub fn build(&self) -> Result<(Document, Vec<T>), DocumentBuilderError> {
        let mut graph = self.construct_graph()?;
        self.reduce_document(&mut graph)
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
        document_view_id: DocumentViewId,
    ) -> Result<(Document, Vec<T>), DocumentBuilderError> {
        let mut graph = self.construct_graph()?;
        // Trim the graph to the requested view..
        graph = graph.trim(document_view_id.graph_tips())?;
        self.reduce_document(&mut graph)
    }

    /// Construct the document graph.
    fn construct_graph(&self) -> Result<Graph<OperationId, T>, DocumentBuilderError> {
        // Instantiate the graph.
        let mut graph = Graph::new();

        let mut create_seen = false;

        // Add all operations to the graph.
        for operation in &self.0 {
            // Check if this is a create operation and we already saw one, this should trigger an error.
            if operation.is_create() && create_seen {
                return Err(DocumentBuilderError::MultipleCreateOperations);
            };

            // Set the operation_seen flag.
            if operation.is_create() {
                create_seen = true;
            }

            graph.add_node(operation.id(), operation.to_owned());
        }

        // Add links between operations in the graph.
        for operation in &self.0 {
            if let Some(previous) = operation.previous() {
                for previous in previous.iter() {
                    let success = graph.add_link(previous, operation.id());
                    if !success {
                        return Err(DocumentBuilderError::InvalidOperationLink(
                            operation.id().to_owned(),
                        ));
                    }
                }
            }
        }

        Ok(graph)
    }

    /// Traverse the graph, visiting operations in their topologically sorted order and reduce
    /// them into a single document.
    fn reduce_document(
        &self,
        graph: &mut Graph<OperationId, T>,
    ) -> Result<(Document, Vec<T>), DocumentBuilderError> {
        // Walk the graph, visiting nodes in their topologically sorted order.
        //
        // We pass in a DocumentReducer which will construct the document as nodes (which contain
        // operations) are visited.
        let mut document_reducer = DocumentReducer::default();
        let graph_data = graph.reduce(&mut document_reducer)?;
        let graph_tips: Vec<OperationId> = graph_data
            .current_graph_tips()
            .iter()
            .map(|operation| operation.id().to_owned())
            .collect();

        // Unwrap the document as if no error occurred it should be there.
        let mut document = document_reducer.document.unwrap();

        // One remaining task is to set the current document view id of the document. This is
        // required as the document reducer only knows about the operations it visits in their
        // already sorted order. It doesn't know about the state of the graphs tips.
        document.view_id = DocumentViewId::new(&graph_tips);

        Ok((document, graph_data.sorted()))
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::traits::AsDocument;
    use crate::document::{DocumentId, DocumentViewFields, DocumentViewId, DocumentViewValue};
    use crate::hash::{Hash, HashId};
    use crate::identity::KeyPair;
    use crate::operation::header::HeaderAction;
    use crate::operation::traits::Identifiable;
    use crate::operation::{OperationBuilder, OperationId, OperationValue};
    use crate::schema::{FieldType, Schema, SchemaId, SchemaName};
    use crate::test_utils::fixtures::{
        document_id, document_view_id, key_pair, random_document_view_id, random_hash, schema,
        schema_id,
    };
    use crate::Human;

    use super::DocumentBuilder;

    const TIMESTAMP: u128 = 17037976940000000;

    #[rstest]
    fn string_representation(key_pair: KeyPair, schema_id: SchemaId) {
        let operation = OperationBuilder::new(&schema_id, TIMESTAMP)
            .fields(&[("name", "Panda".into())])
            .timestamp(1703027623)
            .sign(&key_pair)
            .unwrap();

        let (document, _) = DocumentBuilder::new(vec![operation]).build().unwrap();

        assert_eq!(
            document.to_string(),
            "a095a6a3dbbc17f1ab96bd8479a7742082d656f4bf352fda82872e17518c562b"
        );

        // Short string representation
        assert_eq!(document.display(), "<Document 8c562b>");

        // Make sure the id is matching
        assert_eq!(
            document.id().as_str(),
            "a095a6a3dbbc17f1ab96bd8479a7742082d656f4bf352fda82872e17518c562b"
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

        let mut operations = Vec::new();

        // Panda publishes a CREATE operation.
        // This instantiates a new document.
        //
        // DOCUMENT: [panda_1]

        let panda_operation_1 = OperationBuilder::new(schema.id(), TIMESTAMP)
            .fields(&[("name", OperationValue::String("Panda Cafe".to_string()))])
            .timestamp(1703027623)
            .sign(&panda)
            .unwrap();

        let document_id = DocumentId::new(panda_operation_1.id());

        operations.push(panda_operation_1.clone());

        // Panda publishes an UPDATE operation.
        // It contains the id of the previous operation in it's `previous` array
        //
        // DOCUMENT: [panda_1]<--[panda_2]
        //

        let panda_operation_2 = OperationBuilder::new(schema.id(), TIMESTAMP + 1)
            .document_id(&document_id)
            .backlink(panda_operation_1.id().as_hash())
            .previous(&panda_operation_1.id().clone().into())
            .timestamp(1703027624)
            .depth(1)
            .fields(&[("name", OperationValue::String("Panda Cafe!".to_string()))])
            .sign(&panda)
            .unwrap();

        operations.push(panda_operation_2.clone());

        // Penguin publishes an update operation which creates a new branch in the graph.
        // This is because they didn't know about Panda's second operation.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]
        //                    \----[panda_2]

        let penguin_operation_1 = OperationBuilder::new(schema.id(), TIMESTAMP + 2)
            .document_id(&document_id)
            .previous(&panda_operation_1.id().clone().into())
            .timestamp(1703027625)
            .depth(1)
            .fields(&[(
                "name",
                OperationValue::String("Penguin Cafe!!!".to_string()),
            )])
            .sign(&penguin)
            .unwrap();

        operations.push(penguin_operation_1.clone());

        // Penguin publishes a new operation while now being aware of the previous branching situation.
        // Their `previous` field now contains 2 operation id's.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]<---[penguin_2]
        //                    \----[panda_2]<--/

        let penguin_operation_2 = OperationBuilder::new(schema.id(), TIMESTAMP + 3)
            .document_id(&document_id)
            .backlink(penguin_operation_1.id().as_hash())
            .previous(&DocumentViewId::new(&[
                penguin_operation_1.id().clone(),
                panda_operation_2.id().clone(),
            ]))
            .timestamp(1703027626)
            .depth(2)
            .fields(&[(
                "name",
                OperationValue::String("Polar Bear Cafe".to_string()),
            )])
            .sign(&penguin)
            .unwrap();

        operations.push(penguin_operation_2.clone());

        // Penguin publishes a new update operation which points at the current graph tip.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]<---[penguin_2]<--[penguin_3]
        //                    \----[panda_2]<--/

        let penguin_operation_3 = OperationBuilder::new(schema.id(), TIMESTAMP + 4)
            .document_id(&document_id)
            .backlink(penguin_operation_2.id().as_hash())
            .previous(&penguin_operation_2.id().clone().into())
            .timestamp(1703027627)
            .depth(3)
            .fields(&[(
                "name",
                OperationValue::String("Polar Bear Cafe!!!!!!!!!!".to_string()),
            )])
            .sign(&penguin)
            .unwrap();

        operations.push(penguin_operation_3.clone());

        let (document, operations) = DocumentBuilder::new(operations).build().unwrap();
        let mut exp_result = DocumentViewFields::new();
        exp_result.insert(
            "name",
            DocumentViewValue::new(
                penguin_operation_3.id().into(),
                &OperationValue::String("Polar Bear Cafe!!!!!!!!!!".to_string()),
            ),
        );

        let document_id = DocumentId::new(panda_operation_1.id().into());
        let expected_graph_tips: Vec<OperationId> = vec![penguin_operation_3.id().clone()];

        assert_eq!(
            document.fields().unwrap().get("name"),
            exp_result.get("name")
        );
        assert!(document.is_edited());
        assert!(!document.is_deleted());
        assert_eq!(document.author(), &panda.public_key());
        assert_eq!(document.schema_id(), schema.id());
        assert_eq!(document.view_id().graph_tips(), expected_graph_tips);
        assert_eq!(document.id(), &document_id);

        // Multiple documents receiving operations in different orders should resolve to same value.
        let (document_1, _) = DocumentBuilder::new(vec![
            operations[4].clone(),
            operations[3].clone(),
            operations[2].clone(),
            operations[1].clone(),
            operations[0].clone(),
        ])
        .build()
        .unwrap();

        let (document_2, _) = DocumentBuilder::new(vec![
            operations[2].clone(),
            operations[1].clone(),
            operations[0].clone(),
            operations[4].clone(),
            operations[3].clone(),
        ])
        .build()
        .unwrap();

        assert_eq!(
            document_1.fields().unwrap().get("name"),
            exp_result.get("name")
        );
        assert!(document_1.is_edited());
        assert!(!document_1.is_deleted());
        assert_eq!(document_1.author(), &panda.public_key());
        assert_eq!(document_1.schema_id(), schema.id());
        assert_eq!(document_1.view_id().graph_tips(), expected_graph_tips);
        assert_eq!(document_1.id(), &document_id);

        assert_eq!(
            document_1.fields().unwrap().get("name"),
            document_2.fields().unwrap().get("name")
        );
        assert_eq!(document_1.id(), document_2.id());
        assert_eq!(
            document_1.view_id().graph_tips(),
            document_2.view_id().graph_tips(),
        );
    }

    #[rstest]
    fn must_have_create_operation(
        key_pair: KeyPair,
        schema_id: SchemaId,
        document_id: DocumentId,
        #[from(random_document_view_id)] previous: DocumentViewId,
        #[from(random_hash)] backlink: Hash,
    ) {
        let fields = vec![
            ("firstname", "Peter".into()),
            ("lastname", "Panda".into()),
            ("year", 2020.into()),
        ];

        let update_operation = OperationBuilder::new(&schema_id, TIMESTAMP)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&previous)
            .timestamp(1703027623)
            .depth(1)
            .fields(&fields)
            .sign(&key_pair)
            .unwrap();

        let document = DocumentBuilder::new(vec![update_operation.clone()]).build();
        assert_eq!(
            document.unwrap_err().to_string(),
            format!(
                "operation {} cannot be connected to the document graph",
                update_operation.id()
            )
        );
    }

    #[rstest]
    #[tokio::test]
    async fn incorrect_previous_operations(
        key_pair: KeyPair,
        schema_id: SchemaId,
        #[from(random_document_view_id)] document_view_id: DocumentViewId,
    ) {
        let fields = vec![
            ("firstname", "Peter".into()),
            ("lastname", "Panda".into()),
            ("year", 2020.into()),
        ];

        let create_operation = OperationBuilder::new(&schema_id, TIMESTAMP)
            .timestamp(1703027623)
            .fields(&fields)
            .sign(&key_pair)
            .unwrap();

        let update_operation = OperationBuilder::new(&schema_id, TIMESTAMP + 1)
            .document_id(&create_operation.id().clone().into())
            .backlink(&create_operation.id().as_hash())
            .previous(&document_view_id)
            .timestamp(1703027624)
            .depth(1)
            .fields(&fields)
            .sign(&key_pair)
            .unwrap();

        let document = DocumentBuilder::new(vec![update_operation.clone()]).build();

        assert_eq!(
            document.unwrap_err().to_string(),
            format!(
                "operation {} cannot be connected to the document graph",
                update_operation.id()
            )
        );
    }

    #[rstest]
    #[tokio::test]
    async fn operation_schemas_not_matching(
        key_pair: KeyPair,
        schema_id: SchemaId,
        document_view_id: DocumentViewId,
    ) {
        let fields = vec![
            ("firstname", "Peter".into()),
            ("lastname", "Panda".into()),
            ("year", 2020.into()),
        ];

        let create_operation = OperationBuilder::new(&schema_id, TIMESTAMP)
            .timestamp(1703027623)
            .fields(&fields)
            .sign(&key_pair)
            .unwrap();

        let incorrect_schema_id =
            SchemaId::Application(SchemaName::new("my_new_schema").unwrap(), document_view_id);

        let create_operation_id = create_operation.id();
        let update_operation = OperationBuilder::new(&incorrect_schema_id, TIMESTAMP + 1)
            .document_id(&create_operation_id.clone().into())
            .backlink(&create_operation_id.as_hash())
            .previous(&create_operation_id.clone().into())
            .timestamp(1703027624)
            .depth(1)
            .fields(&fields)
            .sign(&key_pair)
            .unwrap();

        let document =
            DocumentBuilder::new(vec![create_operation, update_operation.clone()]).build();

        assert_eq!(
            document.unwrap_err().to_string(),
            "Could not perform reducer function: Operation 5736b0c1834f0adc1604b40a3b9cc0424be92b1973da8071fa72e6d490c077d7 does not match the documents schema".to_string()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn is_deleted(key_pair: KeyPair, schema_id: SchemaId) {
        let fields = vec![
            ("firstname", "Peter".into()),
            ("lastname", "Panda".into()),
            ("year", 2020.into()),
        ];

        let create_operation = OperationBuilder::new(&schema_id, TIMESTAMP)
            .fields(&fields)
            .sign(&key_pair)
            .unwrap();

        let create_operation_id = create_operation.id();
        let delete_operation = OperationBuilder::new(&schema_id, TIMESTAMP + 1)
            .action(HeaderAction::Delete)
            .document_id(&create_operation_id.clone().into())
            .backlink(&create_operation_id.as_hash())
            .previous(&create_operation_id.clone().into())
            .depth(1)
            .sign(&key_pair)
            .unwrap();

        let (document, _) = DocumentBuilder::new(vec![create_operation, delete_operation.clone()])
            .build()
            .unwrap();

        assert!(document.is_deleted());
        assert!(document.fields().is_none());
    }

    #[rstest]
    #[tokio::test]
    async fn more_than_one_create(key_pair: KeyPair, schema_id: SchemaId) {
        let fields = vec![
            ("firstname", "Peter".into()),
            ("lastname", "Panda".into()),
            ("year", 2020.into()),
        ];

        let create_operation_1 = OperationBuilder::new(&schema_id, TIMESTAMP)
            .fields(&fields)
            .sign(&key_pair)
            .unwrap();

        let create_operation_2 = OperationBuilder::new(&schema_id, TIMESTAMP)
            .fields(&fields)
            .sign(&key_pair)
            .unwrap();

        let result = DocumentBuilder::new(vec![create_operation_1, create_operation_2]).build();

        assert_eq!(
            result.unwrap_err().to_string(),
            "multiple CREATE operations found when building operation graph".to_string()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn fields(#[with(vec![("name".to_string(), FieldType::String)])] schema: Schema) {
        let panda = KeyPair::from_private_key_str(
            "ddcafe34db2625af34c8ba3cf35d46e23283d908c9848c8b43d1f5d0fde779ea",
        )
        .unwrap();

        let penguin = KeyPair::from_private_key_str(
            "1c86b2524b48f0ba86103cddc6bdfd87774ab77ab4c0ea989ed0eeab3d28827a",
        )
        .unwrap();

        let mut operations = Vec::new();

        // Panda publishes a CREATE operation.
        // This instantiates a new document.
        //
        // DOCUMENT: [panda_1]

        let panda_operation_1 = OperationBuilder::new(schema.id(), TIMESTAMP)
            .timestamp(1703027623)
            .fields(&[("name", OperationValue::String("Panda Cafe".to_string()))])
            .sign(&panda)
            .unwrap();

        let document_id = DocumentId::new(panda_operation_1.id());

        operations.push(panda_operation_1.clone());

        // Panda publishes an UPDATE operation.
        // It contains the id of the previous operation in it's `previous` array
        //
        // DOCUMENT: [panda_1]<--[panda_2]
        //

        let panda_operation_2 = OperationBuilder::new(schema.id(), TIMESTAMP + 1)
            .document_id(&document_id)
            .backlink(panda_operation_1.id().as_hash())
            .previous(&panda_operation_1.id().clone().into())
            .timestamp(1703027624)
            .depth(1)
            .fields(&[("name", OperationValue::String("Panda Cafe!".to_string()))])
            .sign(&panda)
            .unwrap();

        operations.push(panda_operation_2.clone());

        // Penguin publishes an update operation which creates a new branch in the graph.
        // This is because they didn't know about Panda's second operation.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]
        //                    \----[panda_2]

        let penguin_operation_1 = OperationBuilder::new(schema.id(), TIMESTAMP + 2)
            .document_id(&document_id)
            .previous(&panda_operation_2.id().clone().into())
            .depth(1)
            .timestamp(1703027625)
            .fields(&[(
                "name",
                OperationValue::String("Penguin Cafe!!!".to_string()),
            )])
            .sign(&penguin)
            .unwrap();

        operations.push(penguin_operation_1.clone());

        let document_builder = DocumentBuilder::new(operations);

        let (document, _) = document_builder
            .build_to_view_id(panda_operation_1.id().clone().into())
            .unwrap();
        assert_eq!(
            document.fields().unwrap().get("name").unwrap().value(),
            &OperationValue::String("Panda Cafe".to_string())
        );

        let (document, _) = document_builder
            .build_to_view_id(panda_operation_2.id().clone().into())
            .unwrap();
        assert_eq!(
            document.fields().unwrap().get("name").unwrap().value(),
            &OperationValue::String("Panda Cafe!".to_string())
        );

        let (document, _) = document_builder
            .build_to_view_id(penguin_operation_1.id().clone().into())
            .unwrap();
        assert_eq!(
            document.fields().unwrap().get("name").unwrap().value(),
            &OperationValue::String("Penguin Cafe!!!".to_string())
        );

        let (document, _) = document_builder
            .build_to_view_id(DocumentViewId::new(&[
                panda_operation_2.id().clone(),
                penguin_operation_1.id().clone(),
            ]))
            .unwrap();
        assert_eq!(
            document.fields().unwrap().get("name").unwrap().value(),
            &OperationValue::String("Penguin Cafe!!!".to_string())
        );
    }

    #[rstest]
    #[tokio::test]
    async fn apply_commit(#[with(vec![("name".to_string(), FieldType::String)])] schema: Schema) {
        let panda = KeyPair::from_private_key_str(
            "ddcafe34db2625af34c8ba3cf35d46e23283d908c9848c8b43d1f5d0fde779ea",
        )
        .unwrap();

        let mut operations = Vec::new();

        let create_operation = OperationBuilder::new(schema.id(), TIMESTAMP)
            .fields(&[("name", OperationValue::String("Panda Cafe".to_string()))])
            .sign(&panda)
            .unwrap();

        let document_id = DocumentId::new(create_operation.id());

        let update_operation = OperationBuilder::new(schema.id(), TIMESTAMP + 1)
            .document_id(&document_id)
            .backlink(create_operation.id().as_hash())
            .previous(&create_operation.id().clone().into())
            .fields(&[("name", OperationValue::String("Panda Cafe!".to_string()))])
            .depth(1)
            .sign(&panda)
            .unwrap();

        operations.push(update_operation.clone());

        let delete_operation = OperationBuilder::new(schema.id(), TIMESTAMP + 2)
            .action(HeaderAction::Delete)
            .document_id(&document_id)
            .backlink(update_operation.id().as_hash())
            .previous(&update_operation.id().clone().into())
            .depth(2)
            .sign(&panda)
            .unwrap();

        operations.push(delete_operation.clone());

        // Create the initial document from a single CREATE operation.
        let (mut document, _) = DocumentBuilder::new(vec![create_operation.clone()])
            .build()
            .unwrap();

        assert!(!document.is_edited());
        assert_eq!(
            document.view_id(),
            &DocumentViewId::from(create_operation.id().clone())
        );
        assert_eq!(
            document.get("name").unwrap(),
            &OperationValue::String("Panda Cafe".to_string())
        );

        // Apply a commit with an UPDATE operation.
        document.commit(&update_operation).unwrap();

        assert!(document.is_edited());
        assert_eq!(
            document.view_id(),
            &DocumentViewId::from(update_operation.id().clone())
        );
        assert_eq!(
            document.get("name").unwrap(),
            &OperationValue::String("Panda Cafe!".to_string())
        );

        // Apply a commit with a DELETE operation.
        document.commit(&delete_operation).unwrap();

        assert!(document.is_deleted());
        assert_eq!(
            document.view_id(),
            &DocumentViewId::from(delete_operation.id().clone())
        );
        assert_eq!(document.fields(), None);
    }

    #[rstest]
    #[tokio::test]
    async fn validate_commit_operation(
        key_pair: KeyPair,
        schema_id: SchemaId,
        #[from(random_document_view_id)] schema_view_id: DocumentViewId,
        #[from(random_document_view_id)] incorrect_previous: DocumentViewId,
    ) {
        let create_operation = OperationBuilder::new(&schema_id, TIMESTAMP)
            .fields(&[("name", OperationValue::String("Panda Cafe".to_string()))])
            .sign(&key_pair)
            .unwrap();

        let create_operation_id = create_operation.id();

        // Create the initial document from a single CREATE operation.
        let (mut document, _) = DocumentBuilder::new(vec![create_operation.clone()])
            .build()
            .unwrap();

        // Committing a CREATE operation should fail.
        assert!(document.commit(&create_operation).is_err());

        // Apply a commit with an UPDATE operation containing the wrong schema id.
        let incorrect_schema_id =
            SchemaId::Application(SchemaName::new("my_new_schema").unwrap(), schema_view_id);
        let update_operation_incorrect_schema_id =
            OperationBuilder::new(&incorrect_schema_id, TIMESTAMP + 1)
                .document_id(&create_operation_id.clone().into())
                .backlink(&create_operation_id.as_hash())
                .previous(&create_operation_id.clone().into())
                .fields(&[("name", OperationValue::String("Panda Cafe!".to_string()))])
                .depth(1)
                .sign(&key_pair)
                .unwrap();

        assert!(document
            .commit(&update_operation_incorrect_schema_id)
            .is_err());

        // Apply a commit with an UPDATE operation not pointing to the current view.
        let update_not_referring_to_current_view = OperationBuilder::new(&schema_id, TIMESTAMP + 1)
            .document_id(&create_operation_id.clone().into())
            .backlink(&create_operation_id.as_hash())
            .previous(&incorrect_previous)
            .fields(&[("name", OperationValue::String("Panda Cafe!".to_string()))])
            .depth(1)
            .sign(&key_pair)
            .unwrap();

        assert!(document
            .commit(&update_not_referring_to_current_view)
            .is_err());

        // Now we apply a correct delete operation.
        let delete_operation = OperationBuilder::new(&schema_id, TIMESTAMP + 1)
            .action(HeaderAction::Delete)
            .document_id(&create_operation_id.clone().into())
            .backlink(&create_operation_id.as_hash())
            .previous(&create_operation_id.clone().into())
            .depth(1)
            .sign(&key_pair)
            .unwrap();

        assert!(document.commit(&delete_operation).is_ok());

        // Apply a commit with an UPDATE operation on a deleted document.
        let delete_view_id = DocumentViewId::new(&[delete_operation.id().clone()]);
        let update_on_a_deleted_document = OperationBuilder::new(&schema_id, TIMESTAMP + 2)
            .document_id(&create_operation_id.clone().into())
            .backlink(&create_operation_id.as_hash())
            .previous(&delete_view_id)
            .depth(2)
            .fields(&[("name", OperationValue::String("Panda Cafe!".to_string()))])
            .sign(&key_pair)
            .unwrap();

        assert!(document.commit(&update_on_a_deleted_document).is_err());
    }
}
