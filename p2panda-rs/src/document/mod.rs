// SPDX-License-Identifier: AGPL-3.0-or-later

//! High-level datatype representing data published to the p2panda network as key-value pairs.
//!
//! Documents are multi-writer and have automatic conflict resolution strategies which produce deterministic
//! state for any two replicas. The underlying structure which make this possible is a directed acyclic graph
//! of [`Operation`]'s. To arrive at the current state of a document the graph is topologically sorted,
//! with any branches being ordered according to the conflicting operations [`OperationId`]. Each operation's
//! mutation is applied in order which results in a LWW (last write wins) resolution strategy.
//!
//! All documents have an accomapanying `Schema` which describes the shape of the data they will contain. Every
//! operation should have been validated aginst this schema before being included in the graph.
//!
//! Documents are constructed through the [`DocumentBuilder`] or by conversion from vectors of a type implementing
//! the [`AsOperation`], [`WithId<OperationId>`] and [`WithPublicKey`].
//!
//! ## Example
//!
//! ```
//! # extern crate p2panda_rs;
//! #
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # #[cfg(feature = "test-utils")]
//! # {
//! # use p2panda_rs::operation::{OperationBuilder, OperationAction, OperationValue};
//! # use p2panda_rs::document::{DocumentBuilder, DocumentViewValue};
//! # use p2panda_rs::document::traits::AsDocument;
//! # use p2panda_rs::identity::KeyPair;
//! # use p2panda_rs::schema::{SchemaId, SchemaName};
//! # use p2panda_rs::test_utils::fixtures::{random_operation_id, random_document_view_id};
//! #
//! # let schema_name = SchemaName::new("cafe")?;
//! # let schema_id = SchemaId::Application(schema_name, random_document_view_id());
//! # let public_key = KeyPair::new().public_key().to_owned();
//! # let operation_id = random_operation_id();
//! #
//! // Construct a CREATE operation.
//! let operation_value = OperationValue::String("Panda Cafe".to_string());
//! let operation = OperationBuilder::new(&schema_id)
//!     .action(OperationAction::Create)
//!     .fields(&[("name", operation_value.clone())])
//!     .build()
//!     .unwrap();
//!
//! // Build a document from a single operation, we include it's id and the public key of the
//! // author who published it.
//! let document = DocumentBuilder::new(vec![(operation_id.clone(), operation, public_key)]).build()?;
//! // The document view value contains the value we expect for the "name" field as well
//! // as the id of the operation which last updated this field.
//! assert_eq!(
//!     document.view().unwrap().get("name"),
//!     Some(&DocumentViewValue::new(&operation_id, &operation_value))
//! );
//! # }
//! # Ok(())
//! # }
//! ```
//!
//! As can be seen in this example a tuple of ([`OperationId`], [`Operation`], [`PublicKey`]) is required for each operation
//! included in the graph. The [`OperationId`] and [`PublicKey`] are both derived from the signed [`Entry`] the operation was
//! published on.
//!
//! A useful characteristic of documents is the ability to view state from any point in the past (as long as the
//! operations have been retained, see below for details on this). Every state a document has passed through can
//! be identified by a [`DocumentViewId`] which is the id's of the current graph tip operations. This id can be
//! used to request a view onto any state from the documents past.]
//!
//! ## Example
//!
//! ```
//! # extern crate p2panda_rs;
//! #
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # #[cfg(feature = "test-utils")]
//! # {
//! # use p2panda_rs::operation::{OperationBuilder, OperationAction, OperationValue};
//! # use p2panda_rs::document::{DocumentBuilder, DocumentViewId, DocumentViewValue};
//! # use p2panda_rs::document::traits::AsDocument;
//! # use p2panda_rs::identity::KeyPair;
//! # use p2panda_rs::schema::{SchemaId, SchemaName};
//! # use p2panda_rs::test_utils::fixtures::{random_operation_id, random_document_view_id};
//! #
//! # let schema_name = SchemaName::new("cafe")?;
//! # let schema_id = SchemaId::Application(schema_name, random_document_view_id());
//! # let public_key = KeyPair::new().public_key().to_owned();
//! # let operation_id_1 = random_operation_id();
//! # let operation_id_2 = random_operation_id();
//! #
//! // Construct a CREATE operation.
//! let operation_1_value = OperationValue::String("Panda Cafe".to_string());
//! let operation_1 = OperationBuilder::new(&schema_id)
//!     .action(OperationAction::Create)
//!     .fields(&[("name", operation_1_value.clone())])
//!     .build()
//!     .unwrap();
//!
//! // Construct an UPDATE operation.
//! let operation_2_value = OperationValue::String("Polar Bear Cafe".to_string());
//! let operation_2 = OperationBuilder::new(&schema_id)
//!     .action(OperationAction::Update)
//!     .fields(&[("name", operation_2_value.clone())])
//!     .previous(&DocumentViewId::new(&[operation_id_1.clone()]))
//!     .build()
//!     .unwrap();
//!
//! // Construct a document builder from these two operations.
//! let document_builder = DocumentBuilder::new(vec![
//!     (operation_id_1.clone(), operation_1, public_key),
//!     (operation_id_2.clone(), operation_2, public_key),
//! ]);
//!
//! // Build the document to it's latest view.
//! let document = document_builder.build().unwrap();
//!
//! // The document view value contains the value we expect for the "name" field as well
//! // as the id of the operation which last updated this field.
//! assert_eq!(
//!     document.view().unwrap().get("name"),
//!     Some(&DocumentViewValue::new(&operation_id_2, &operation_2_value))
//! );
//!
//! // Derive a document view id for the initial document state.
//! let document_view_id_1 = DocumentViewId::new(&[operation_id_1.clone()]);
//! // Build the document again but to this earlier state.
//! let document = document_builder.build_to_view_id(Some(document_view_id_1)).unwrap();
//!
//! assert_eq!(
//!     document.view().unwrap().get("name"),
//!     Some(&DocumentViewValue::new(&operation_id_1, &operation_1_value))
//! )
//! # }
//! # Ok(())
//! # }
//!
//! ```
#[allow(clippy::module_inception)]
mod document;
mod document_id;
mod document_view;
mod document_view_fields;
mod document_view_hash;
mod document_view_id;
pub mod error;
pub mod traits;

pub use document::{Document, DocumentBuilder};
pub use document_id::DocumentId;
pub use document_view::DocumentView;
pub use document_view_fields::{DocumentViewFields, DocumentViewValue};
pub use document_view_hash::DocumentViewHash;
pub use document_view_id::DocumentViewId;
