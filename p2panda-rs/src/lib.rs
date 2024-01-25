// SPDX-License-Identifier: AGPL-3.0-or-later

//! # p2panda-rs
//!
//! This library provides all tools required to write a client for the [p2panda] network. It is
//! shipped both as a Rust crate `p2panda-rs` with WebAssembly bindings and a NPM package
//! `p2panda-js` with TypeScript definitions running in NodeJS or any modern web browser.
//!
//! [p2panda]: https://p2panda.org
//!
//! ## Example
//!
//! Creates and signs data which can be sent to a p2panda node.
//!
//! ```
//! # extern crate p2panda_rs;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # use std::convert::TryFrom;
//! # use p2panda_rs::entry::EntryBuilder;
//! # use p2panda_rs::entry::encode::encode_entry;
//! # use p2panda_rs::hash::Hash;
//! # use p2panda_rs::identity::KeyPair;
//! # use p2panda_rs::operation::{OperationBuilder, OperationId};
//! # use p2panda_rs::operation::encode::encode_operation;
//! # use p2panda_rs::schema::{SchemaId, SchemaName};
//! #
//! # let schema_name = SchemaName::new("profile")?;
//! # let view_id = OperationId::from(Hash::new_from_bytes(&[1, 2, 3]));
//! # let profile_schema_id = SchemaId::new_application(&schema_name, &view_id.into());
//! // Generate new Ed25519 key pair
//! let key_pair = KeyPair::new();
//!
//! // Add field data to "create" operation
//! let operation = OperationBuilder::new(&profile_schema_id)
//!     .fields(&[("username", "panda".into())])
//!     .build()?;
//!
//! // Encode operation into bytes
//! let encoded_operation = encode_operation(&operation)?;
//!
//! // Create Bamboo entry (append-only log data type) with operation as payload
//! let entry = EntryBuilder::new()
//!     .sign(&encoded_operation, &key_pair)?;
//!
//! // Encode entry into bytes
//! let encoded_entry = encode_entry(&entry)?;
//!
//! println!("{} {}", encoded_entry, encoded_operation);
//! # Ok(())
//! # }
//! ```
#![warn(
    missing_debug_implementations,
    rustdoc::missing_doc_code_examples,
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]
// This must be imported here at the root of the crate in order for the `rstest` fixture macros to
// work as expected.
#![allow(clippy::single_component_path_imports, clippy::uninlined_format_args)]
#[cfg(any(feature = "test-utils", test))]
use rstest_reuse;

#[cfg(any(feature = "storage-provider", test))]
pub mod api;
pub mod document;
pub mod graph;
pub mod hash;
pub mod identity;
pub mod operation;
pub mod schema;
#[cfg(feature = "secret-group")]
pub mod secret_group;
pub mod serde;
#[cfg(any(feature = "storage-provider", test))]
pub mod storage_provider;
#[cfg(any(feature = "test-utils", test))]
pub mod test_utils;
#[cfg(target_arch = "wasm32")]
pub mod wasm;

/// Trait used by p2panda structs to validate data formats.
///
/// Use this trait to check against (canonic) formats of data (like document ids or yasmf hashes)
/// coming in via deserialization, constructors or (string) conversion.
pub trait Validate {
    /// Validation error type.
    type Error: std::fmt::Debug + std::error::Error + Send + Sync + 'static;

    /// Validates p2panda data type instance.
    fn validate(&self) -> Result<(), Self::Error>;
}

/// Trait used by p2panda structs for human-facing functionality, like better readability.
///
/// Please note: Most structs already provide string representation methods which can be used for
/// debugging with additional type information (`Debug`) or lossless string representations of the
/// data (`Display`). `Display` implementations return a string which can safely be parsed back
/// into the struct again. `Human` takes a third approach which is potentially destructive and aims
/// at easier to read strings.
pub trait Human {
    /// Returns a shorter representation of the type.
    ///
    /// Since p2panda values can at times be very long (for example hashes) this method can be used
    /// to implement a shorter representation of the value, which is destructive but easier to read
    /// for humans (and not computers).
    fn display(&self) -> String;
}

/// Trait used by p2panda structs which contain at least one id.
///
/// A single struct may have several id's, common use cases will be `WithId<OperationId>`,
/// `WithId<DocumentId>` and `WithId<SchemaId>`.
pub trait WithId<T> {
    /// Returns the identifier for this operation.
    fn id(&self) -> &T;
}

/// Init pretty_env_logger before the test suite runs to handle logging outputs.
///
/// We output log information using the `log` crate. In itself this doesn't print
/// out any logging information, library users can capture and handle the emitted logs
/// using a log handler. Here we use `pretty_env_logger` to handle logs emitted
/// while running our tests.
///
/// This will also capture and output any logs emitted from our dependencies. This behaviour
/// can be customised at runtime. With eg. `RUST_LOG=p2panda=info cargo t` or
/// `RUST_LOG=openmls=debug cargo t`.
///
/// The `ctor` crate is used to define a global constructor function. This method
/// will be run before any of the test suites.
#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
#[ctor::ctor]
fn init() {
    // If the `RUST_LOG` env var is not set skip initiation as we don't want
    // to see any logs.
    if std::env::var("RUST_LOG").is_ok() {
        pretty_env_logger::init();
    }
}
