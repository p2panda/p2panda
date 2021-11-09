// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::{decode_entry, EntrySigned};
use crate::identity::Author;
use crate::materializer::{Edge, MaterializationError};
use crate::message::MessageEncoded;

pub type InstanceAuthor = Author;

/// Method for marshalling an array of (EntrySigned, MessageEncoded) into an array of Edges which can then
/// be turned into a DAG. It is here to detach the DAG from any concepts of Entries and Messages. The DAG just sees
/// Strings. This step will look different once have some more instance related fields in our Message struct.
pub fn marshall_entries(
    entries: Vec<(EntrySigned, MessageEncoded)>,
) -> Result<Vec<Edge>, MaterializationError> {
    let mut edges = Vec::new();
    for (entry_signed, message_encoded) in entries {
        let entry = match decode_entry(&entry_signed, Some(&message_encoded)) {
            Ok(entry) => Ok(entry),
            Err(err) => Err(MaterializationError::EntrySignedError(err)),
        }?;

        if entry.message().is_none() {
            // The message has been deleted.
            continue;
        }

        // If we have a `link` field in the Message then we can use it here.
        // `id` should not be optional (even CREATE messages should have it set) then
        // we wouldn't need the EntrySigned here at all.
        let (link, id) = match entry.message().unwrap().id() {
            Some(_) => (
                Some(entry.backlink_hash().unwrap().as_str().to_owned()),
                entry_signed.hash().as_str().to_owned(),
            ),
            None => (None, entry_signed.hash().as_str().to_owned()),
        };
        edges.push((link, id));
    }
    Ok(edges)
}

/// Filter entries against instance author for a single writer setting. This is needed for materializing System Logs.
pub fn single_writer(
    entries: Vec<(EntrySigned, MessageEncoded, InstanceAuthor)>,
) -> Vec<(EntrySigned, MessageEncoded, InstanceAuthor)> {
    entries
        .iter()
        .cloned()
        .filter(|(entry_encoded, _, instance_author)| {
            entry_encoded.author().as_str() == instance_author.as_str()
        })
        .collect()
}

/// Filter entries against permissions for multi writer setting. This is needed for materializing User Logs which allow
/// update messages from multiple writers via the use of permissions.
pub fn multi_writer(
    entries: Vec<(EntrySigned, MessageEncoded, InstanceAuthor)>,
    permitted_authors: Vec<Author>,
) -> Vec<(EntrySigned, MessageEncoded, InstanceAuthor)> {
    entries
        .iter()
        .cloned()
        .filter(|(entry_encoded, _, instance_author)| {
            entry_encoded.author().as_str() == instance_author.as_str()
                || permitted_authors.iter().any(|permitted_author| {
                    permitted_author.as_str() == entry_encoded.author().as_str()
                })
        })
        .collect()
}
