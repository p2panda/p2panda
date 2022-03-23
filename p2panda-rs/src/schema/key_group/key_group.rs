// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::convert::TryFrom;

use crate::document::{Document, DocumentId, DocumentView, DocumentViewId};
use crate::identity::Author;
use crate::operation::OperationValue;
use crate::schema::system::SystemSchemaError;
use crate::Validate;

use super::error::KeyGroupError;
use super::{Membership, MembershipRequestView, MembershipView, Owner};

/// Represents a group of key pairs that can be assigned shared ownership of documents.
#[derive(Debug)]
pub struct KeyGroup {
    id: DocumentId,
    name: String,
    members: HashMap<Author, Membership>,
}

impl KeyGroup {
    /// Create a key group from documents of itself, its memberships and membership requests.
    pub fn new(
        key_group: Document,
        requests: &[Document],
        responses: &[Document],
    ) -> Result<KeyGroup, KeyGroupError> {
        let key_group_view = KeyGroupView::try_from(key_group.view().clone())?;
        let requests: Vec<MembershipRequestView> = requests
            .iter()
            .map(|doc| MembershipRequestView::try_from(doc.clone()).unwrap())
            .collect();
        let responses: Vec<MembershipView> = responses
            .iter()
            .map(|doc| MembershipView::try_from(doc.clone()).unwrap())
            .collect();

        let mut members: Vec<Membership> = Vec::new();
        for response in responses {
            match requests
                .iter()
                .find(|request| request.view_id() == response.request())
            {
                Some(request) => {
                    members.push(Membership::new(request.clone(), response.clone())?);
                }
                None => {
                    continue;
                }
            };
        }
        KeyGroup::new_from_members(key_group.id().clone(), key_group_view, &members)
    }

    /// Create a key group from a key group view and a set of memberships.
    pub fn new_from_members(
        id: DocumentId,
        key_group_view: KeyGroupView,
        members: &[Membership],
    ) -> Result<KeyGroup, KeyGroupError> {
        let mut member_map: HashMap<Author, Membership> = HashMap::new();
        for membership in members {
            let new_val = match membership.member() {
                Owner::Author(value) => member_map.insert(value.clone(), membership.clone()),
                Owner::KeyGroup(_) => {
                    todo!("requires access to storage for getting that key group's members")
                }
            };
            if new_val.is_some() {
                return Err(KeyGroupError::DuplicateMembership(format!(
                    "{:?}",
                    membership
                )));
            }
        }

        let key_group = KeyGroup {
            id,
            name: key_group_view.name().to_string(),
            members: member_map,
        };

        key_group.validate()?;
        Ok(key_group)
    }

    /// Returns the key group's id.
    pub fn id(&self) -> &DocumentId {
        todo!()
    }

    /// Access the key group's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Test whether an [`Author`] is a member.
    pub fn is_member(&self, author: &Author) -> bool {
        match self.members.get(author) {
            Some(membership) => *membership.accepted(),
            None => false,
        }
    }

    /// Get the membership for an [`Author`].
    pub fn get(&self, author: &Author) -> Option<&Membership> {
        self.members.get(author)
    }
}

impl Validate for KeyGroup {
    type Error = KeyGroupError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.members.is_empty() {
            return Err(KeyGroupError::NoMemberships);
        }
        Ok(())
    }
}

/// Represents a root key group definition.
///
/// Can be used to make a [`KeyGroup`].
#[derive(Debug)]
pub struct KeyGroupView {
    view_id: DocumentViewId,
    name: String,
}

#[allow(dead_code)]
impl KeyGroupView {
    /// The id of this key group view.
    pub fn view_id(&self) -> &DocumentViewId {
        &self.view_id
    }

    /// The name of this key group.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl TryFrom<DocumentView> for KeyGroupView {
    type Error = SystemSchemaError;

    fn try_from(document_view: DocumentView) -> Result<Self, Self::Error> {
        let name = match document_view.get("name") {
            Some(OperationValue::Text(value)) => Ok(value),
            Some(op) => Err(SystemSchemaError::InvalidField(
                "name".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("name".to_string())),
        }?;

        Ok(Self {
            view_id: document_view.id().clone(),
            name: name.clone(),
        })
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use rstest::rstest;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::identity::{Author, KeyPair};
    use crate::operation::{PinnedRelation, Relation, RelationList, OperationValue};
    use crate::schema::key_group::Membership;
    use crate::test_utils::fixtures::{create_operation, fields, random_key_pair};
    use crate::test_utils::mocks::{send_to_node, Client, Node};
    use crate::test_utils::utils::update_operation;

    use super::{KeyGroup, KeyGroupView};

    #[rstest]
    fn no_members(random_key_pair: KeyPair) {
        let frog = Client::new("frog".to_string(), random_key_pair);
        let mut node = Node::new();

        let (key_group_doc_id, _) = send_to_node(
            &mut node,
            &frog,
            &create_operation(
                "key_group_v1".parse().unwrap(),
                fields(vec![(
                    "name",
                    OperationValue::Text("Strawberry Picking Gang".to_string()),
                )]),
            ),
        )
        .unwrap();

        let document = node.get_document(&key_group_doc_id);
        let view = KeyGroupView::try_from(document.view().clone()).unwrap();
        let members: Vec<Membership> = vec![];
        assert_eq!(
            format!(
                "{}",
                KeyGroup::new_from_members(key_group_doc_id.into(), view, &members).unwrap_err()
            ),
            "key group must have at least one member"
        );
    }

    #[rstest]
    fn key_group_creation(
        #[from(random_key_pair)] frog_key_pair: KeyPair,
        #[from(random_key_pair)] rabbit_key_pair: KeyPair,
    ) {
        let frog = Client::new("frog".to_string(), frog_key_pair);
        let frog_author = Author::new(&frog.public_key()).unwrap();

        let rabbit = Client::new("rabbit".to_string(), rabbit_key_pair);
        let rabbit_author = Author::new(&rabbit.public_key()).unwrap();

        let mut node = Node::new();

        // Frog creates the 'Strawberry Picking Gang' key group
        let (key_group_id, _) = send_to_node(
            &mut node,
            &frog,
            &create_operation(
                "key_group_v1".parse().unwrap(),
                fields(vec![
                    (
                        "name",
                        OperationValue::Text("Strawberry Picking Gang".to_string()),
                    ),
                ]),
            ),
        )
        .unwrap();

        // ... and makes herself a member
        let (frog_request_doc_id, _) = send_to_node(
            &mut node,
            &frog,
            &create_operation(
                "key_group_membership_request_v1".parse().unwrap(),
                fields(vec![(
                    "key_group",
                    OperationValue::Relation(Relation::new(DocumentId::new(
                        key_group_id.clone().into(),
                    ))),
                )]),
            ),
        )
        .unwrap();

        let frog_request =
            node.get_document(&frog_request_doc_id);

        let (frog_membership_doc_id, _) = send_to_node(
            &mut node,
            &frog,
            &create_operation(
                "key_group_membership_v1".parse().unwrap(),
                fields(vec![
                    (
                        "request",
                        OperationValue::PinnedRelation(PinnedRelation::new(DocumentViewId::from(
                            frog_request_doc_id.clone(),
                        ))),
                    ),
                    ("accepted", OperationValue::Boolean(true)),
                ]),
            ),
        )
        .unwrap();
        let frog_response = node.get_document(&frog_membership_doc_id);

        let key_group = KeyGroup::new(
            node.get_document(&key_group_id),
            &[frog_request.clone()],
            &[frog_response.clone()],
        )
        .unwrap();

        assert!(key_group.is_member(&frog_author));

        // Rabbit asks to become a member as well
        let (rabbit_request_doc_id, _) = send_to_node(
            &mut node,
            &rabbit,
            &create_operation(
                "key_group_membership_request_v1".parse().unwrap(),
                fields(vec![(
                    "key_group",
                    OperationValue::Relation(Relation::new(DocumentId::new(
                        key_group_id.clone().into(),
                    ))),
                )]),
            ),
        )
        .unwrap();
        let rabbit_request =
            node.get_document(&rabbit_request_doc_id);

        // But rabbit is not a member yet
        let key_group = KeyGroup::new(
            node.get_document(&key_group_id),
            &[frog_request.clone(), rabbit_request.clone()],
            &[frog_response.clone()],
        )
        .unwrap();

        assert!(!key_group.is_member(&rabbit_author));

        // Now frog let's rabbit in :)
        let (rabbit_membership_doc_id, _) = send_to_node(
            &mut node,
            &frog,
            &create_operation(
                "key_group_membership_v1".parse().unwrap(),
                fields(vec![
                    (
                        "request",
                        OperationValue::PinnedRelation(PinnedRelation::new(
                            rabbit_request.view_id().clone(),
                        )),
                    ),
                    ("accepted", OperationValue::Boolean(true)),
                ]),
            ),
        )
        .unwrap();

        let rabbit_response = node.get_document(&rabbit_membership_doc_id);

        let key_group = KeyGroup::new(
            node.get_document(&key_group_id),
            &[frog_request.clone(), rabbit_request.clone()],
            &[frog_response.clone(), rabbit_response.clone()],
        )
        .unwrap();

        assert!(key_group.is_member(&rabbit_author));

        // But rabbit would rather pick strawberries alone.
        send_to_node(
            &mut node,
            &rabbit,
            &update_operation(
                "key_group_membership_v1".parse().unwrap(),
                vec![frog_membership_doc_id.clone().into()],
                fields(vec![("accepted", OperationValue::Boolean(false))]),
            ),
        )
        .unwrap();

        let frog_response = node.get_document(&frog_membership_doc_id);

        let key_group = KeyGroup::new(
            node.get_document(&key_group_id),
            &[frog_request, rabbit_request],
            &[frog_response, rabbit_response],
        )
        .unwrap();

        assert!(!key_group.is_member(&frog_author));
    }
}
