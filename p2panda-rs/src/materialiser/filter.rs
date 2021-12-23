// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::EntrySigned;
use crate::identity::Author;
use crate::operation::OperationEncoded;

/// Filter entries against instance author for a single writer setting. This is needed for
/// materializing system Logs.
#[allow(dead_code)]
pub fn single_writer_filter(
    entries: Vec<(EntrySigned, OperationEncoded)>,
    instance_author: Author,
) -> Vec<(EntrySigned, OperationEncoded)> {
    entries
        .iter()
        .cloned()
        .filter(|(entry_encoded, _)| entry_encoded.author().as_str() == instance_author.as_str())
        .collect()
}

/// Filter entries against permissions for multi writer setting. This is needed for materializing
/// application logs which allow update operations from multiple writers via the use of
/// permissions.
#[allow(dead_code)]
pub fn multi_writer_filter(
    entries: Vec<(EntrySigned, OperationEncoded)>,
    instance_author: Author,
    permitted_authors: Vec<Author>,
) -> Vec<(EntrySigned, OperationEncoded)> {
    entries
        .iter()
        .cloned()
        .filter(|(entry_encoded, _)| {
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

    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::OperationValue;
    use crate::test_utils::fixtures::{
        create_operation, fields, random_key_pair, schema, update_operation,
    };
    use crate::test_utils::mocks::Client;
    use crate::test_utils::mocks::{send_to_node, Node};

    use super::{multi_writer_filter, single_writer_filter};

    #[rstest]
    fn filtering_tests(
        schema: Hash,
        #[from(random_key_pair)] key_pair_1: KeyPair,
        #[from(random_key_pair)] key_pair_2: KeyPair,
    ) {
        let panda = Client::new("panda".to_string(), key_pair_1);
        let penguin = Client::new("penguin".to_string(), key_pair_2);
        let mut node = Node::new();

        let panda_entry_1_hash = send_to_node(
            &mut node,
            &panda,
            &create_operation(
                schema.clone(),
                fields(vec![(
                    "message",
                    OperationValue::Text("Hello!".to_string()),
                )]),
            ),
        )
        .unwrap();

        send_to_node(
            &mut node,
            &panda,
            &update_operation(
                schema.clone(),
                panda_entry_1_hash.clone(),
                fields(vec![(
                    "message",
                    OperationValue::Text("Hello too!".to_string()),
                )]),
            ),
        )
        .unwrap();

        send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                schema,
                panda_entry_1_hash,
                fields(vec![(
                    "message",
                    OperationValue::Text("Hello too!".to_string()),
                )]),
            ),
        )
        .unwrap();

        let entries = node.all_entries();
        let entry_1 = entries.get(0).unwrap();
        let entry_2 = entries.get(1).unwrap();
        let entry_3 = entries.get(2).unwrap();
        let formatted_entries = vec![
            (entry_1.entry_encoded(), entry_1.operation_encoded()),
            (entry_2.entry_encoded(), entry_2.operation_encoded()),
            (entry_3.entry_encoded(), entry_3.operation_encoded()),
        ];

        let single_writer_entries = single_writer_filter(formatted_entries.clone(), panda.author());

        assert_eq!(single_writer_entries.len(), 2);

        let multi_writer_entries_without_permission =
            multi_writer_filter(formatted_entries.clone(), panda.author(), vec![]);

        assert_eq!(multi_writer_entries_without_permission.len(), 2);

        let multi_writer_entries_with_permission =
            multi_writer_filter(formatted_entries, panda.author(), vec![penguin.author()]);

        assert_eq!(multi_writer_entries_with_permission.len(), 3);
    }
}
