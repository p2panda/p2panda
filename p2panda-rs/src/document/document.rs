// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::{Debug, Display};

use crate::document::error::{DocumentBuilderError, DocumentReducerError};
use crate::document::traits::AsDocument;
use crate::document::{DocumentId, DocumentViewFields, DocumentViewId};
use crate::graph::{Graph, Reducer};
use crate::identity::PublicKey;
use crate::operation::traits::AsOperation;
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
/// the [`AsOperation`], [`WithId<OperationId>`] and [`WithPublicKey`].
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
impl<T: AsOperation> Reducer<T> for DocumentReducer {
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
    T: AsOperation + Debug + Clone + PartialEq,
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
    use std::convert::{TryFrom, TryInto};

    use rstest::rstest;

    use crate::document::traits::AsDocument;
    use crate::document::{
        Document, DocumentId, DocumentViewFields, DocumentViewId, DocumentViewValue,
    };
    use crate::hash::HashId;
    use crate::identity::KeyPair;
    use crate::operation::traits::AsOperation;
    use crate::operation::{OperationAction, OperationBuilder, OperationId, OperationValue};
    use crate::schema::{FieldType, Schema, SchemaId, SchemaName};
    use crate::test_utils::constants::{self, PRIVATE_KEY};
    use crate::test_utils::fixtures::{
        key_pair, operation_fields, random_document_view_id, random_operation_id, schema, schema_id,
    };
    use crate::test_utils::memory_store::helpers::send_to_store;
    use crate::test_utils::memory_store::MemoryStore;
    use crate::{Human, WithId};

    use super::DocumentBuilder;

    #[rstest]
    fn string_representation(key_pair: KeyPair, schema_id: SchemaId) {
        let operation = OperationBuilder::new(&schema_id, 1703027623)
            .fields(&[("name", "Panda".into())])
            .sign(&key_pair)
            .unwrap();

        let (document, _) = DocumentBuilder::new(vec![operation]).build().unwrap();

        assert_eq!(
            document.to_string(),
            "e03a60b773ab2a16642a39b11a23e779017d32ec01e557eb50d9963cac9d2096"
        );

        // Short string representation
        assert_eq!(document.display(), "<Document 9d2096>");

        // Make sure the id is matching
        assert_eq!(
            document.id().as_str(),
            "e03a60b773ab2a16642a39b11a23e779017d32ec01e557eb50d9963cac9d2096"
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

        let panda_operation_1 = OperationBuilder::new(schema.id(), 1703027623)
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

        let panda_operation_2 = OperationBuilder::new(schema.id(), 1703027624)
            .document_id(&document_id)
            .backlink(panda_operation_1.id().as_hash())
            .previous(&panda_operation_1.id().clone().into())
            .fields(&[("name", OperationValue::String("Panda Cafe!".to_string()))])
            .sign(&panda)
            .unwrap();

        operations.push(panda_operation_2.clone());

        // Penguin publishes an update operation which creates a new branch in the graph.
        // This is because they didn't know about Panda's second operation.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]
        //                    \----[panda_2]

        let penguin_operation_1 = OperationBuilder::new(schema.id(), 1703027625)
            .document_id(&document_id)
            .fields(&[(
                "name",
                OperationValue::String("Penguin Cafe!!!".to_string()),
            )])
            .previous(&panda_operation_2.id().clone().into())
            .sign(&panda)
            .unwrap();

        operations.push(penguin_operation_1.clone());

        // Penguin publishes a new operation while now being aware of the previous branching situation.
        // Their `previous` field now contains 2 operation id's.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]<---[penguin_2]
        //                    \----[panda_2]<--/

        let penguin_operation_2 = OperationBuilder::new(schema.id(), 1703027626)
            .document_id(&document_id)
            .backlink(penguin_operation_1.id().as_hash())
            .previous(&DocumentViewId::new(&[
                penguin_operation_1.id().clone(),
                panda_operation_2.id().clone(),
            ]))
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

        let penguin_operation_3 = OperationBuilder::new(schema.id(), 1703027627)
            .document_id(&document_id)
            .backlink(penguin_operation_2.id().as_hash())
            .previous(&penguin_operation_2.id().clone().into())
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
    //
    //     #[rstest]
    //     fn must_have_create_operation(
    //         #[from(published_operation)]
    //         #[with(
    //             Some(operation_fields(constants::test_fields())),
    //             constants::schema(),
    //             Some(random_document_view_id())
    //         )]
    //         update_operation: PublishedOperation,
    //     ) {
    //         let document: Result<Document, _> = vec![&update_operation].try_into();
    //         assert_eq!(
    //             document.unwrap_err().to_string(),
    //             format!(
    //                 "operation {} cannot be connected to the document graph",
    //                 WithId::<OperationId>::id(&update_operation)
    //             )
    //         );
    //     }
    //
    //     #[rstest]
    //     #[tokio::test]
    //     async fn incorrect_previous_operations(
    //         #[from(published_operation)]
    //         #[with(Some(operation_fields(constants::test_fields())), constants::schema())]
    //         create_operation: PublishedOperation,
    //         #[from(published_operation)]
    //         #[with(
    //             Some(operation_fields(constants::test_fields())),
    //             constants::schema(),
    //             Some(random_document_view_id())
    //         )]
    //         update_operation: PublishedOperation,
    //     ) {
    //         let document: Result<Document, _> = vec![&create_operation, &update_operation].try_into();
    //
    //         assert_eq!(
    //             document.unwrap_err().to_string(),
    //             format!(
    //                 "operation {} cannot be connected to the document graph",
    //                 WithId::<OperationId>::id(&update_operation).clone()
    //             )
    //         );
    //     }
    //
    //     #[rstest]
    //     #[tokio::test]
    //     async fn operation_schemas_not_matching() {
    //         let create_operation = published_operation(
    //             Some(operation_fields(constants::test_fields())),
    //             constants::schema(),
    //             None,
    //             KeyPair::from_private_key_str(PRIVATE_KEY).unwrap(),
    //         );
    //
    //         let update_operation = published_operation(
    //             Some(operation_fields(vec![
    //                 ("name", "is_cute".into()),
    //                 ("type", "bool".into()),
    //             ])),
    //             Schema::get_system(SchemaId::SchemaFieldDefinition(1))
    //                 .unwrap()
    //                 .to_owned(),
    //             Some(WithId::<OperationId>::id(&create_operation).clone().into()),
    //             KeyPair::from_private_key_str(PRIVATE_KEY).unwrap(),
    //         );
    //
    //         let document: Result<Document, _> = vec![&create_operation, &update_operation].try_into();
    //
    //         assert_eq!(
    //             document.unwrap_err().to_string(),
    //             "Could not perform reducer function: Operation 0020b7674a56756183f7d2c6afa20e06041a9a9a30b0aec728e35acf281ecff2b544 does not match the documents schema".to_string()
    //         );
    //     }
    //
    //     #[rstest]
    //     #[tokio::test]
    //     async fn is_deleted(
    //         #[from(published_operation)]
    //         #[with(Some(operation_fields(constants::test_fields())), constants::schema())]
    //         create_operation: PublishedOperation,
    //     ) {
    //         let delete_operation = published_operation(
    //             None,
    //             constants::schema(),
    //             Some(DocumentViewId::new(&[WithId::<OperationId>::id(
    //                 &create_operation,
    //             )
    //             .clone()])),
    //             KeyPair::from_private_key_str(PRIVATE_KEY).unwrap(),
    //         );
    //
    //         let document: Document = vec![&create_operation, &delete_operation]
    //             .try_into()
    //             .unwrap();
    //
    //         assert!(document.is_deleted());
    //         assert!(document.fields().is_none());
    //     }
    //
    //     #[rstest]
    //     #[tokio::test]
    //     async fn more_than_one_create(
    //         #[from(published_operation)] create_operation: PublishedOperation,
    //     ) {
    //         let document: Result<Document, _> = vec![&create_operation, &create_operation].try_into();
    //
    //         assert_eq!(
    //             document.unwrap_err().to_string(),
    //             "multiple CREATE operations found when building operation graph".to_string()
    //         );
    //     }
    //
    //     #[rstest]
    //     #[tokio::test]
    //     async fn fields(#[with(vec![("name".to_string(), FieldType::String)])] schema: Schema) {
    //         let mut operations = Vec::new();
    //
    //         let panda = KeyPair::new().public_key().to_owned();
    //         let penguin = KeyPair::new().public_key().to_owned();
    //
    //         // Panda publishes a CREATE operation.
    //         // This instantiates a new document.
    //         //
    //         // DOCUMENT: [panda_1]
    //
    //         let operation_1_id = random_operation_id();
    //         let operation = OperationBuilder::new(schema.id())
    //             .action(OperationAction::Create)
    //             .fields(&[("name", OperationValue::String("Panda Cafe".to_string()))])
    //             .build()
    //             .unwrap();
    //
    //         operations.push((operation_1_id.clone(), operation, panda));
    //
    //         // Panda publishes an UPDATE operation.
    //         // It contains the id of the previous operation in it's `previous` array
    //         //
    //         // DOCUMENT: [panda_1]<--[panda_2]
    //         //
    //
    //         let operation_2_id = random_operation_id();
    //         let operation = OperationBuilder::new(schema.id())
    //             .action(OperationAction::Update)
    //             .fields(&[("name", OperationValue::String("Panda Cafe!".to_string()))])
    //             .previous(&DocumentViewId::new(&[operation_1_id.clone()]))
    //             .build()
    //             .unwrap();
    //
    //         operations.push((operation_2_id.clone(), operation, panda));
    //
    //         // Penguin publishes an update operation which creates a new branch in the graph.
    //         // This is because they didn't know about Panda's second operation.
    //         //
    //         // DOCUMENT: [panda_1]<--[penguin_1]
    //         //                    \----[panda_2]
    //
    //         let operation_3_id = random_operation_id();
    //         let operation = OperationBuilder::new(schema.id())
    //             .action(OperationAction::Update)
    //             .fields(&[(
    //                 "name",
    //                 OperationValue::String("Penguin Cafe!!!".to_string()),
    //             )])
    //             .previous(&DocumentViewId::new(&[operation_2_id.clone()]))
    //             .build()
    //             .unwrap();
    //
    //         operations.push((operation_3_id.clone(), operation, penguin));
    //
    //         let document_builder = DocumentBuilder::new(operations);
    //
    //         let (document, _) = document_builder
    //             .build_to_view_id(DocumentViewId::new(&[operation_1_id]))
    //             .unwrap();
    //         assert_eq!(
    //             document.fields().unwrap().get("name").unwrap().value(),
    //             &OperationValue::String("Panda Cafe".to_string())
    //         );
    //
    //         let (document, _) = document_builder
    //             .build_to_view_id(DocumentViewId::new(&[operation_2_id.clone()]))
    //             .unwrap();
    //         assert_eq!(
    //             document.fields().unwrap().get("name").unwrap().value(),
    //             &OperationValue::String("Panda Cafe!".to_string())
    //         );
    //
    //         let (document, _) = document_builder
    //             .build_to_view_id(DocumentViewId::new(&[operation_3_id.clone()]))
    //             .unwrap();
    //         assert_eq!(
    //             document.fields().unwrap().get("name").unwrap().value(),
    //             &OperationValue::String("Penguin Cafe!!!".to_string())
    //         );
    //
    //         let (document, _) = document_builder
    //             .build_to_view_id(DocumentViewId::new(&[operation_2_id, operation_3_id]))
    //             .unwrap();
    //         assert_eq!(
    //             document.fields().unwrap().get("name").unwrap().value(),
    //             &OperationValue::String("Penguin Cafe!!!".to_string())
    //         );
    //     }
    //
    //     #[rstest]
    //     #[tokio::test]
    //     async fn apply_commit(
    //         #[from(published_operation)]
    //         #[with(Some(operation_fields(constants::test_fields())), constants::schema())]
    //         create_operation: PublishedOperation,
    //     ) {
    //         // Construct operations we will use to update an existing document.
    //
    //         let create_view_id =
    //             DocumentViewId::new(&[WithId::<OperationId>::id(&create_operation).clone()]);
    //
    //         let update_operation = operation(
    //             Some(operation_fields(vec![("age", OperationValue::Integer(21))])),
    //             Some(create_view_id.clone()),
    //             constants::schema().id().to_owned(),
    //         );
    //
    //         let update_operation_id = random_operation_id();
    //         let update_view_id = DocumentViewId::new(&[update_operation_id.clone()]);
    //
    //         let delete_operation = operation(
    //             None,
    //             Some(update_view_id.clone()),
    //             constants::schema().id().to_owned(),
    //         );
    //
    //         let delete_operation_id = random_operation_id();
    //         let delete_view_id = DocumentViewId::new(&[delete_operation_id.clone()]);
    //
    //         // Create the initial document from a single CREATE operation.
    //         let mut document: Document = vec![&create_operation].try_into().unwrap();
    //
    //         assert!(!document.is_edited());
    //         assert_eq!(document.view_id(), &create_view_id);
    //         assert_eq!(document.get("age").unwrap(), &OperationValue::Integer(28));
    //
    //         // Apply a commit with an UPDATE operation.
    //         document
    //             .commit(&update_operation_id, &update_operation)
    //             .unwrap();
    //
    //         assert!(document.is_edited());
    //         assert_eq!(document.view_id(), &update_view_id);
    //         assert_eq!(document.get("age").unwrap(), &OperationValue::Integer(21));
    //
    //         // Apply a commit with a DELETE operation.
    //         document
    //             .commit(&delete_operation_id, &delete_operation)
    //             .unwrap();
    //
    //         assert!(document.is_deleted());
    //         assert_eq!(document.view_id(), &delete_view_id);
    //         assert_eq!(document.fields(), None);
    //     }
    //
    //     #[rstest]
    //     #[tokio::test]
    //     async fn validate_commit_operation(
    //         #[from(published_operation)]
    //         #[with(Some(operation_fields(constants::test_fields())), constants::schema())]
    //         create_operation: PublishedOperation,
    //     ) {
    //         // Create the initial document from a single CREATE operation.
    //         let mut document: Document = vec![&create_operation].try_into().unwrap();
    //
    //         // Committing a CREATE operation should fail.
    //         assert!(document
    //             .commit(create_operation.id(), &create_operation)
    //             .is_err());
    //
    //         let create_view_id =
    //             DocumentViewId::new(&[WithId::<OperationId>::id(&create_operation).clone()]);
    //
    //         let schema_name = SchemaName::new("my_wrong_schema").expect("Valid schema name");
    //         let update_with_incorrect_schema_id = published_operation(
    //             Some(operation_fields(vec![("age", OperationValue::Integer(21))])),
    //             schema(
    //                 vec![("age".into(), FieldType::Integer)],
    //                 SchemaId::new_application(&schema_name, &random_document_view_id()),
    //                 "Schema with a wrong id",
    //             ),
    //             Some(create_view_id.clone()),
    //             KeyPair::from_private_key_str(PRIVATE_KEY).unwrap(),
    //         );
    //
    //         // Apply a commit with an UPDATE operation containing the wrong schema id.
    //         assert!(document
    //             .commit(
    //                 update_with_incorrect_schema_id.id(),
    //                 &update_with_incorrect_schema_id
    //             )
    //             .is_err());
    //
    //         let update_not_referring_to_current_view = published_operation(
    //             Some(operation_fields(vec![("age", OperationValue::Integer(21))])),
    //             constants::schema(),
    //             Some(random_document_view_id()),
    //             KeyPair::from_private_key_str(PRIVATE_KEY).unwrap(),
    //         );
    //
    //         // Apply a commit with an UPDATE operation not pointing to the current view.
    //         assert!(document
    //             .commit(
    //                 update_not_referring_to_current_view.id(),
    //                 &update_not_referring_to_current_view
    //             )
    //             .is_err());
    //
    //         // Now we apply a correct delete operation.
    //         let delete_operation = published_operation(
    //             None,
    //             constants::schema(),
    //             Some(create_view_id.clone()),
    //             KeyPair::from_private_key_str(PRIVATE_KEY).unwrap(),
    //         );
    //
    //         assert!(document
    //             .commit(delete_operation.id(), &delete_operation)
    //             .is_ok());
    //
    //         let delete_view_id =
    //             DocumentViewId::new(&[WithId::<OperationId>::id(&delete_operation).clone()]);
    //
    //         let update_on_a_deleted_document = published_operation(
    //             Some(operation_fields(vec![("age", OperationValue::Integer(21))])),
    //             constants::schema(),
    //             Some(delete_view_id.to_owned()),
    //             KeyPair::from_private_key_str(PRIVATE_KEY).unwrap(),
    //         );
    //
    //         // Apply a commit with an UPDATE operation on a deleted document.
    //         assert!(document
    //             .commit(
    //                 update_on_a_deleted_document.id(),
    //                 &update_on_a_deleted_document
    //             )
    //             .is_err());
    //     }
}
