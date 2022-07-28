// SPDX-License-Identifier: AGPL-3.0-or-later

//! Create, encode and decode p2panda operations.
//!
//! Operations describe data mutations in the p2panda network. Authors send operations to create,
//! update or delete documents.
//!
//! Every operations contains application data which is formed after a schema. To be able to decode
//! an operation, a schema aids with getting the data out of the operation and validation.
//!
//! ## Decoding
//!
//! There are two approaches (similar to `Entry`) to create an `Operation`. Both of them require
//! you to have a `Schema` instance at hand as there is no other way to find out the types of the
//! operation fields.
//!
//! To programmatically create an `Operation`, use the `OperationBuilder`, when working with
//! operations coming in as bytes, you can use the `decode_operation` method to first deserialize
//! it into a `PlainOperation` instance, which is a schemaless object giving you already access to
//! the "header" data, like the schema id.
//!
//! Knowing the schema id you can look up your internal database for known schemas and derive a
//! `Schema` instance from there. Now together with the `PlainOperation` and `Schema` you can
//! finally validate the operation (via `validate_operation`) to arrive at the final, verified
//! `Operation`.
//!
//! ```text
//!              ┌────────────────┐
//!              │OperationBuilder├──────────build()──────────────┐
//!              └────────────────┘            ▲                  │
//!                                            │                  │
//!                                            │                  │
//!                                            │                  ▼
//!                                         ┌──┴───┐          ┌─────────┐
//!                                         │Schema│          │Operation│
//!                                         └──┬───┘          └─────────┘
//!                                            │                  ▲
//!                           Lookup Schema    │                  │
//!                                            │                  │
//!              ┌──────────────┐              ▼                  │
//!              │PlainOperation├───────validate_operation()──────┘
//!              └──────────────┘
//!                     ▲
//!                     │
//!             decode_operation()
//!                     │
//!             ┌───────┴────────┐
//! bytes ────► │EncodedOperation│
//!             └────────────────┘
//! ```
//!
//! Please note that `Operation` in itself is immutable and can not directly be deserialized, there
//! are only these above mentioned approaches to arrive at it. Both approaches apply all means to
//! validate the integrity, schema and correct encoding of the operation as per specification.
//!
//! ## Encoding
//!
//! `Operation` structs can be encoded again into their raw bytes form like that, for this no
//! `Schema` is required:
//!
//! ```text
//! ┌─────────┐                           ┌────────────────┐
//! │Operation│ ───encode_operation()───► │EncodedOperation│ ────► bytes
//! └─────────┘                           └────────────────┘
//! ```
//!
//! ## Validation
//!
//! The above mentioned high-level methods will automatically do different sorts of validation
//! checks. All low-level methods can also be used independently, depending on your implementation:
//!
//! 1. @TODO ..
//! 2. @TODO ..
//!
//! This module also provides a high-level method `validate_operation_and_entry` which will apply
//! _all_ checks required to verify the integrity of an operation and entry. This includes all
//! validation steps listed above plus the ones mentioned in the `entry` module. Since this
//! validation requires you to provide a `Schema` instance and the regarding back- & skiplink
//! `Entry` instances yourself, it needs some preparation from your end which can roughly be
//! described like this:
//!
//! ```text
//!                                                                  Look-Up
//!
//!             ┌────────────┐                       ┌─────┐    ┌─────┐    ┌─────┐
//!  bytes ───► │EncodedEntry├────decode_entry()────►│Entry│    │Entry│    │Entry│
//!             └──────┬─────┘                       └──┬──┘    └─────┘    └─────┘
//!                    │                                │
//!                    └───────────────────────────┐    │       Skiplink   Backlink
//!                                                │    │          │          │
//!             ┌────────────────┐                 │    │          │          │
//!  bytes ───► │EncodedOperation├─────────────┐   │    │          │          │
//!             └───────┬────────┘             │   │    │          │          │
//!                     │                      │   │    │          │          │
//!             decode_operation()             │   │    │          │          │
//!                     │            Look-Up   │   │    │          │          │
//!                     ▼                      │   │    │          │          │
//!              ┌──────────────┐    ┌──────┐  │   │    │          │          │
//!              │PlainOperation│    │Schema│  │   │    │          │          │
//!              └──────┬───────┘    └──┬───┘  │   │    │          │          │
//!                     │               │      │   │    │          │          │
//!                     │               │      │   │    │          │          │
//!                     │               │      │   │    │          │          │
//!                     │               │      │   │    │          │          │
//!                     │               ▼      ▼   ▼    ▼          ▼          │
//!                     └───────────►  validate_operation_and_entry() ◄───────┘
//!                                                 │
//!                                                 │
//!                                                 │
//!                                                 │
//!                                                 ▼
//!                                         ┌─────────────────┐
//!                                         │VerifiedOperation│
//!                                         └─────────────────┘
//! ```
pub mod decode;
pub mod encode;
mod encoded_operation;
pub mod error;
#[allow(clippy::module_inception)]
mod operation;
mod operation_action;
mod operation_fields;
mod operation_id;
mod operation_value;
mod operation_version;
pub mod plain;
mod relation;
pub mod traits;
pub mod validate;
mod verified_operation;

pub use encoded_operation::EncodedOperation;
pub use operation::{Operation, OperationBuilder};
pub use operation_action::OperationAction;
pub use operation_fields::OperationFields;
pub use operation_id::OperationId;
pub use operation_value::OperationValue;
pub use operation_version::OperationVersion;
pub use relation::{PinnedRelation, PinnedRelationList, Relation, RelationList};
pub use verified_operation::VerifiedOperation;
