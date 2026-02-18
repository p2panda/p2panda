// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::error::Error;

/// Maps a topic to a user defined data type being sent over the wire during sync.
///
/// It defines the type of data it is expecting to sync and how the scope for a particular session
/// should be identified; users provide an implementation of the `TopicStore` trait in order to
/// define how this mapping occurs.
///
/// Since `TopicStore` is generic we can use the same mapping across different sync
/// implementations for the same data type when necessary.
///
/// For example a `TopicStore` map implementation could map a generic `T` to a set of logs.
///
/// ## Designing `TopicStore` for applications
///
/// Considering an example chat application which is based on append-only log data types, we
/// probably want to organise messages from an author for a certain chat group into one log each.
/// Like this, a chat group can be expressed as a collection of one to potentially many logs (one
/// per member of the group):
///
/// ```text
/// All authors: A, B and C
/// All chat groups: 1 and 2
///
/// "Chat group 1 with members A and B"
/// - Log A1
/// - Log B1
///
/// "Chat group 2 with members A, B and C"
/// - Log A2
/// - Log B2
/// - Log C2
/// ```
///
/// If we implement `T` to express that we're interested in syncing over a specific chat group,
/// for example "Chat Group 2" we would implement `TopicStore` to give us all append-only logs of
/// all members inside this group, that is the entries inside logs `A2`, `B2` and `C2`.
pub trait TopicStore<T, A, ID> {
    type Error: Error;

    /// Associate an author and data id pair with a topic.
    fn associate(
        &self,
        topic: &T,
        author: &A,
        data_id: &ID,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Remove an association with a topic.
    fn remove(
        &self,
        topic: &T,
        author: &A,
        data_id: &ID,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Get identifiers for all associated
    fn resolve(&self, topic: &T) -> impl Future<Output = Result<HashMap<A, Vec<ID>>, Self::Error>>;
}
