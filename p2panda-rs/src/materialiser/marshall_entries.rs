// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::{decode_entry, EntrySigned};
use crate::materialiser::{Edge, MaterialisationError};
use crate::message::MessageEncoded;

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

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::materialiser::Edge;
    use crate::test_utils::fixtures::{create_message, fields, key_pair, schema, update_message};
    use crate::test_utils::mocks::client::Client;
    use crate::test_utils::mocks::node::{send_to_node, Node};

    use super::marshall_entries;

    #[rstest]
    fn marshall_entries_test(schema: Hash, key_pair: KeyPair) {
        let client = Client::new("panda".to_string(), key_pair);
        let mut node = Node::new();

        let entry_1_hash = send_to_node(
            &mut node,
            &client,
            &create_message(schema.clone(), fields(vec![("message", "Hello!")])),
        )
        .unwrap();

        send_to_node(
            &mut node,
            &client,
            &update_message(
                schema,
                entry_1_hash,
                fields(vec![("message", "Hello too!")]),
            ),
        )
        .unwrap();

        let entries = node.all_entries();
        let entry_1 = entries.get(0).unwrap();
        let entry_2 = entries.get(1).unwrap();

        let edges = marshall_entries(vec![
            (entry_1.entry_encoded(), entry_1.message_encoded()),
            (entry_2.entry_encoded(), entry_2.message_encoded()),
        ])
        .unwrap();

        let edge_1: Edge = (None, entry_1.hash_str());
        let edge_2: Edge = (Some(entry_1.hash_str()), entry_2.hash_str());

        assert_eq!(edges, vec![edge_1, edge_2]);
    }
}
