// SPDX-License-Identifier: AGPL-3.0-or-later

//! Document is a replicatable data type designed to handle concurrent updates in a way where all replicas
//! eventually resolve to the same deterministic value.
//!
//! A Document is made up of a linked graph of operations. During a process of ordering and reduction
//! the graph is resolved to a single data item matching the documents schema definition. Any two documents
//! (replicas) which contain the same collection of operations will resolve to the same value.
//!
//! In the p2panda network, Documents are materialised on nodes and the resultant document views are stored
//! in the database.
//!
//! ```
//! # extern crate p2panda_rs;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # use p2panda_rs::hash::Hash;
//! # use p2panda_rs::identity::KeyPair;
//! # use p2panda_rs::operation::{OperationValue, OperationWithMeta};
//! # use p2panda_rs::schema::SchemaId;
//! # use p2panda_rs::test_utils::utils::{create_operation, delete_operation, update_operation, operation_fields};
//! # use p2panda_rs::test_utils::constants::DEFAULT_SCHEMA_HASH;
//! # use p2panda_rs::test_utils::mocks::{send_to_node, Client, Node};
//! use p2panda_rs::document::DocumentBuilder;
//! #
//! # let polar = Client::new(
//! #     "polar".to_string(),
//! #     KeyPair::from_private_key_str(
//! #         "ddcafe34db2625af34c8ba3cf35d46e23283d908c9848c8b43d1f5d0fde779ea",
//! #     )
//! #     .unwrap(),
//! # );
//! # let panda = Client::new(
//! #     "panda".to_string(),
//! #     KeyPair::from_private_key_str(
//! #         "1c86b2524b48f0ba86103cddc6bdfd87774ab77ab4c0ea989ed0eeab3d28827a",
//! #     )
//! #     .unwrap(),
//! # );
//! #
//! # let schema = SchemaId::new(DEFAULT_SCHEMA_HASH).unwrap();
//! # let mut node = Node::new();
//! #
//! # let (polar_entry_1_hash, _) = send_to_node(
//! #     &mut node,
//! #     &polar,
//! #     &create_operation(
//! #         schema.clone(),
//! #         operation_fields(vec![
//! #             ("name", OperationValue::Text("Polar Bear Cafe".to_string())),
//! #             ("owner", OperationValue::Text("Polar Bear".to_string())),
//! #             ("house-number", OperationValue::Integer(12)),
//! #         ]),
//! #     ),
//! # )
//! # .unwrap();
//! #
//! # let (polar_entry_2_hash, _) = send_to_node(
//! #     &mut node,
//! #     &polar,
//! #     &update_operation(
//! #         schema.clone(),
//! #         vec![polar_entry_1_hash.clone()],
//! #         operation_fields(vec![
//! #             ("name", OperationValue::Text("ʕ •ᴥ•ʔ Cafe!".to_string())),
//! #             ("owner", OperationValue::Text("しろくま".to_string())),
//! #         ]),
//! #     ),
//! # )
//! # .unwrap();
//! #
//! # let (panda_entry_1_hash, _) = send_to_node(
//! #     &mut node,
//! #     &panda,
//! #     &update_operation(
//! #         schema.clone(),
//! #         vec![polar_entry_1_hash.clone()],
//! #         operation_fields(vec![("name", OperationValue::Text("🐼 Cafe!".to_string()))]),
//! #     ),
//! # )
//! # .unwrap();
//! #
//! # let (polar_entry_3_hash, _) = send_to_node(
//! #     &mut node,
//! #     &polar,
//! #     &update_operation(
//! #         schema.clone(),
//! #         vec![panda_entry_1_hash.clone(), polar_entry_2_hash.clone()],
//! #         operation_fields(vec![("house-number", OperationValue::Integer(102))]),
//! #     ),
//! # )
//! # .unwrap();
//! #
//! # let (polar_entry_4_hash, _) = send_to_node(
//! #     &mut node,
//! #     &polar,
//! #     &delete_operation(
//! #         schema,
//! #         vec![polar_entry_3_hash.clone()]
//! #     ),
//! # )
//! # .unwrap();
//! #
//! # let entry_1 = node.get_entry(&polar_entry_1_hash);
//! # let operation_1 =
//! #     OperationWithMeta::new(&entry_1.entry_encoded(), &entry_1.operation_encoded()).unwrap();
//! # let entry_2 = node.get_entry(&polar_entry_2_hash);
//! # let operation_2 =
//! #     OperationWithMeta::new(&entry_2.entry_encoded(), &entry_2.operation_encoded()).unwrap();
//! # let entry_3 = node.get_entry(&panda_entry_1_hash);
//! # let operation_3 =
//! #     OperationWithMeta::new(&entry_3.entry_encoded(), &entry_3.operation_encoded()).unwrap();
//! # let entry_4 = node.get_entry(&polar_entry_3_hash);
//! # let operation_4 =
//! #     OperationWithMeta::new(&entry_4.entry_encoded(), &entry_4.operation_encoded()).unwrap();
//! # let entry_5 = node.get_entry(&polar_entry_4_hash);
//! # let operation_5 =
//! #     OperationWithMeta::new(&entry_5.entry_encoded(), &entry_5.operation_encoded()).unwrap();
//! #
//! //== Operation creation it hidden for brevity, see the operation module docs for details ==//
//!
//! // Here we have a collection of 2 operations
//! let mut operations = vec![
//!     // CREATE operation: {name: "Polar Bear Cafe", owner: "Polar Bear", house-number: 12}
//!     operation_1,
//!     // UPDATE operation: {name: "ʕ •ᴥ•ʔ Cafe!", owner: "しろくま"}
//!     operation_2,
//! ];
//!
//! // These two operations were both published by the same author and they form a simple
//! // update graph which looks like this:
//! //
//! //   ++++++++++++++++++++++++++++    ++++++++++++++++++++++++++++
//! //   | name : "Polar Bear Cafe" |    | name : "ʕ •ᴥ•ʔ Cafe!"    |
//! //   | owner: "Polar Bear"      |<---| owner: "しろくま"　　　　　 |
//! //   | house-number: 12         |    ++++++++++++++++++++++++++++
//! //   ++++++++++++++++++++++++++++
//! //
//! // With these operations we can construct a new document like so:
//! let document = DocumentBuilder::new(operations.clone()).build();
//!
//! // Which is _Ok_ because the collection of operations are valid (there should be exactly
//! // one CREATE operation, they are all causally linked, all operations should follow the
//! // same schema).
//! assert!(document.is_ok());
//! let document = document.unwrap();
//!
//! // This process already builds, sorts and reduces the document. We can now
//! // access the derived view to check it's values.
//!
//! let document_view = document.view();
//!
//! assert_eq!(
//!     document_view.get("name").unwrap(),
//!     &OperationValue::Text("ʕ •ᴥ•ʔ Cafe!".into())
//! );
//! assert_eq!(
//!     document_view.get("owner").unwrap(),
//!     &OperationValue::Text("しろくま".into())
//! );
//! assert_eq!(
//!     document_view.get("house-number").unwrap(),
//!     &OperationValue::Integer(12)
//! );
//!
//! // If another operation arrives, from a different author, which has a causal relation
//! // to the original operation, then we have a new branch in the graph, it might look like
//! // this:
//! //
//! //   ++++++++++++++++++++++++++++    +++++++++++++++++++++++++++
//! //   | name : "Polar Bear Cafe" |    | name :  "ʕ •ᴥ•ʔ Cafe!"  |
//! //   | owner: "Polar Bear"      |<---| owner: "しろくま"　　　　　|
//! //   | house-number: 12         |    +++++++++++++++++++++++++++
//! //   ++++++++++++++++++++++++++++
//! //                A
//! //                |
//! //                |                  +++++++++++++++++++++++++++
//! //                -----------------  | name: "🐼 Cafe!"        |
//! //                                   +++++++++++++++++++++++++++
//! //
//! // This can happen when the document is edited concurrently at different locations, before
//! // either author knew of the others update. It's not a problem though, as a document is
//! // traversed a deterministic path is selected and so two matching collections of operations
//! // will always be sorted into the same order. When there is a conflict (in this case "name"
//! // was changed on both replicas) one of them "just wins" in a last-write-wins fashion.
//!
//! // We can build the document agan now with these 3 operations:
//! //
//! // UPDATE operation: {name: "🐼 Cafe!"}
//! operations.push(operation_3);
//!
//! let document = DocumentBuilder::new(operations.clone()).build().unwrap();
//! let document_view = document.view();
//!
//! // Here we see that "🐼 Cafe!" won the conflict, meaning it was applied after "ʕ •ᴥ•ʔ Cafe!".
//! assert_eq!(
//!     document_view.get("name").unwrap(),
//!     &OperationValue::Text("🐼 Cafe!".into())
//! );
//! assert_eq!(
//!     document_view.get("owner").unwrap(),
//!     &OperationValue::Text("しろくま".into())
//! );
//! assert_eq!(
//!     document_view.get("house-number").unwrap(),
//!     &OperationValue::Integer(12)
//! );
//!
//! // Now our first author publishes a 4th operation after having seen the full collection
//! // of operations. This results in two links to previous operations being formed. Effectively
//! // merging the two graph branches into one again. This is important for retaining update
//! // context. Without it, we wouldn't know the relation between operations existing on
//! // different branches.
//! //
//! //   ++++++++++++++++++++++++++++    +++++++++++++++++++++++++++
//! //   | name : "Polar Bear Cafe" |    | name :  "ʕ •ᴥ•ʔ Cafe!"  |
//! //   | owner: "Polar Bear"      |<---| owner: "しろくま"　　　　　|<---\
//! //   | house-number: 12         |    +++++++++++++++++++++++++++     \
//! //   ++++++++++++++++++++++++++++                                    ++++++++++++++++++++++
//! //                A                                                  | house-number: 102  |
//! //                |                                                  ++++++++++++++++++++++
//! //                |                  +++++++++++++++++++++++++++     /
//! //                -----------------  | name: "🐼 Cafe!"        |<---/
//! //                                   +++++++++++++++++++++++++++
//! //
//!
//! // UPDATE operation: { house-number: 102 }
//! operations.push(operation_4);
//!
//! let document = DocumentBuilder::new(operations.clone()).build().unwrap();
//! let document_view = document.view();
//!
//! assert_eq!(
//!     document_view.get("name").unwrap(),
//!     &OperationValue::Text("🐼 Cafe!".into())
//! );
//! assert_eq!(
//!     document_view.get("owner").unwrap(),
//!     &OperationValue::Text("しろくま".into())
//! );
//! assert_eq!(
//!     document_view.get("house-number").unwrap(),
//!     &OperationValue::Integer(102)
//! );
//!
//! // Finally, we want to delete the document, for this we publish a DELETE operation.
//!
//! // DELETE operation: {}
//! operations.push(operation_5);
//!
//! let document = DocumentBuilder::new(operations.clone()).build().unwrap();
//!
//! assert!(document.is_deleted());
//!
//! # Ok(())
//! # }
//! ```

#[allow(clippy::module_inception)]
mod document;
mod document_id;
mod document_view;
mod document_view_id;
mod error;

pub use document::DocumentBuilder;
#[allow(unused_imports)]
use document::{build_graph, reduce};
pub use document_id::DocumentId;
pub use document_view::DocumentView;
pub use document_view_id::DocumentViewId;
pub use error::{DocumentBuilderError, DocumentError, DocumentViewError};
