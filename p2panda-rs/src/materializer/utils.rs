// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::{decode_entry, EntrySigned};
use crate::materializer::{Edge, MaterializationError};
use crate::message::MessageEncoded;

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
            Err(_) => Err(MaterializationError::Error),
        }?;

        let message = match entry.message() {
            Some(msg) => Ok(msg),
            None => Err(MaterializationError::Error),
        };

        // If we have a `link` field in the Message then we can use it here.
        // `id` should not be optional (even CREATE messages should have it set) then
        // we wouldn't need the EntrySigned here at all.
        let (link, id) = match message?.id() {
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
