// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::{decode_entry, EntrySigned};
use crate::identity::Author;
use crate::materialiser::{Edge, MaterialisationError};
use crate::message::MessageEncoded;

pub type InstanceAuthor = Author;

/// Method for marshalling an array of (EntrySigned, MessageEncoded) into an array of Edges which can then
/// be turned into a DAG. It is here to detach the DAG from any concepts of Entries and Messages. The DAG just sees
/// Strings. This step will look different once have some more instance related fields in our Message struct.
pub fn marshall_entries(
    entries: Vec<(EntrySigned, MessageEncoded)>,
) -> Result<Vec<Edge>, MaterialisationError> {
    let mut edges = Vec::new();
    for (entry_signed, message_encoded) in entries {
        let entry = match decode_entry(&entry_signed, Some(&message_encoded)) {
            Ok(entry) => Ok(entry),
            Err(err) => Err(MaterialisationError::EntrySignedError(err)),
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

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use std::convert::TryFrom;

    use crate::entry::{sign_and_encode, Entry, SeqNum};
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::materialiser::{marshall_entries, Edge};
    use crate::message::MessageEncoded;
    use crate::test_utils::fixtures::{entry, fields, key_pair, schema, update_message};

    #[rstest]
    fn marshall_entries_test(#[from(entry)] entry1: Entry, schema: Hash, key_pair: KeyPair) {
        let encoded_message_1 = MessageEncoded::try_from(entry1.message().unwrap()).unwrap();

        let signed_encoded_entry_1 = sign_and_encode(&entry1, &key_pair).unwrap();

        let message_2 = update_message(
            schema,
            signed_encoded_entry_1.hash(),
            fields(vec![("message", "Hello too!")]),
        );

        let encoded_message_2 = MessageEncoded::try_from(&message_2).unwrap();

        let entry = entry(
            message_2,
            SeqNum::new(2).unwrap(),
            Some(signed_encoded_entry_1.hash()),
            None,
        );

        let signed_encoded_entry_2 = sign_and_encode(&entry, &key_pair).unwrap();

        let edges = marshall_entries(vec![
            (signed_encoded_entry_1.clone(), encoded_message_1),
            (signed_encoded_entry_2.clone(), encoded_message_2),
        ])
        .unwrap();

        let edge_1: Edge = (None, signed_encoded_entry_1.hash().as_str().to_owned());
        let edge_2: Edge = (
            Some(signed_encoded_entry_1.hash().as_str().to_owned()),
            signed_encoded_entry_2.hash().as_str().to_string(),
        );

        assert_eq!(edges, vec![edge_1, edge_2]);
    }
}
