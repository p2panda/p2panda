// SPDX-License-Identifier: MIT OR Apache-2.0

//! `TopicStore` trait for managing mappings of application data to topics as well as a concrete
//! `SqliteStore` implementation.
mod sqlite;
#[cfg(test)]
mod tests;
mod traits;

pub use traits::TopicStore;
