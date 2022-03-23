// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

use crate::document::{Document, DocumentId};
use crate::identity::Author;
use crate::operation::OperationValue;

use super::{KeyGroup, KeyGroupError};

/// Represents the owner of a document, which may be a public key or a key group.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Owner {
    /// Key group owners are represented by the key group's [`DocumentId`].
    KeyGroup(DocumentId),

    /// Public keys are represented as [`Author`].
    Author(Author),
}

impl Owner {
    /// Returns the owner of a [`Document`].
    pub fn for_document(document: &Document) -> Result<Owner, KeyGroupError> {
        /// Returns an [`Owner`] if the parameter contains an `OperationValue::Owner`.
        fn get_owner_values(value: &OperationValue) -> Option<Owner> {
            match value {
                OperationValue::Owner(relation) => {
                    Some(Owner::KeyGroup(relation.document_id().clone()))
                }
                _ => None,
            }
        }

        let owners: Vec<Owner> = document
            .view()
            .iter()
            // Take document view field values
            .map(|(_, value)| value)
            // Map to `Option<Owner>`
            .map(|value| get_owner_values(value))
            // Unwrap
            .filter(|value| value.is_some())
            .map(|value| value.unwrap())
            .collect();

        match owners.len() {
            0 => Ok(document.author().clone().into()),
            1 => Ok(owners[0].clone()),
            _ => Err(KeyGroupError::MultipleOwners(
                document.id().as_str().to_string(),
            )),
        }
    }
}

impl From<KeyGroup> for Owner {
    fn from(key_group: KeyGroup) -> Owner {
        Owner::KeyGroup(key_group.id().clone())
    }
}

impl From<Author> for Owner {
    fn from(author: Author) -> Owner {
        Owner::Author(author)
    }
}

#[cfg(test)]
mod test {
    use rstest::rstest;

    use crate::identity::KeyPair;
    use crate::operation::{OperationId, Relation};
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::{fields, random_key_pair, random_operation_id, schema};
    use crate::test_utils::mocks::{send_to_node, Client, Node};
    use crate::test_utils::utils::create_operation;

    use super::*;

    #[rstest]
    fn author_owner(
        random_key_pair: KeyPair,
        #[from(random_operation_id)] key_group_id: OperationId,
    ) {
        let rabbit = Client::new("rabbit".to_string(), random_key_pair);
        let mut node = Node::new();

        let (rabbit_request_hash, _) = send_to_node(
            &mut node,
            &rabbit,
            &create_operation(
                "key_group_membership_request_v1".parse().unwrap(),
                fields(vec![(
                    "key_group",
                    OperationValue::Relation(Relation::new(DocumentId::new(key_group_id))),
                )]),
            ),
        )
        .unwrap();

        let doc = node.get_document(&rabbit_request_hash);
        let owner = Owner::for_document(&doc).unwrap();
        assert_eq!(
            owner,
            Owner::Author(Author::new(rabbit.public_key().as_str()).unwrap())
        )
    }

    #[rstest]
    fn key_group_owner(
        random_key_pair: KeyPair,
        #[from(random_operation_id)] key_group_id: OperationId,
        schema: SchemaId,
    ) {
        let rabbit = Client::new("rabbit".to_string(), random_key_pair);
        let mut node = Node::new();

        let (rabbit_request_hash, _) = send_to_node(
            &mut node,
            &rabbit,
            &create_operation(
                schema,
                fields(vec![(
                    "parent",
                    OperationValue::Owner(Relation::new(DocumentId::new(key_group_id.clone()))),
                )]),
            ),
        )
        .unwrap();

        let doc = node.get_document(&rabbit_request_hash);
        let owner = Owner::for_document(&doc).unwrap();
        assert_eq!(owner, Owner::KeyGroup(DocumentId::new(key_group_id)));
    }

    #[rstest]
    fn multiple_owners(
        random_key_pair: KeyPair,
        #[from(random_operation_id)] key_group_id: OperationId,
        #[from(random_operation_id)] key_group_id_2: OperationId,
        schema: SchemaId,
    ) {
        let rabbit = Client::new("rabbit".to_string(), random_key_pair);
        let mut node = Node::new();

        let (rabbit_request_hash, _) = send_to_node(
            &mut node,
            &rabbit,
            &create_operation(
                schema,
                fields(vec![
                    (
                        "parent",
                        OperationValue::Owner(Relation::new(DocumentId::new(key_group_id.clone()))),
                    ),
                    (
                        "grandparent",
                        OperationValue::Owner(Relation::new(DocumentId::new(
                            key_group_id_2.clone(),
                        ))),
                    ),
                ]),
            ),
        )
        .unwrap();

        let doc = node.get_document(&rabbit_request_hash);
        let result = Owner::for_document(&doc).unwrap_err();
        assert_eq!(
            format!("{}", result),
            format!(
                "unexpected multiple owner fields in document {}",
                doc.id().as_str()
            )
        );
    }
}
