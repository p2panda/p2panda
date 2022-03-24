// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::convert::TryFrom;

use crate::document::{Document, DocumentId, DocumentViewId};
use crate::identity::Author;
use crate::operation::OperationValue;
use crate::schema::system::SystemSchemaError;
use crate::schema::SchemaId;
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
        key_group_id: DocumentId,
        documents: &[Document],
        member_key_groups: &[KeyGroup],
    ) -> Result<KeyGroup, KeyGroupError> {
        let mut key_group = None;
        let mut requests = vec![];
        let mut responses = vec![];

        for document in documents {
            match document.schema() {
                SchemaId::KeyGroupMembership => {
                    responses.push(MembershipView::try_from(document.clone())?)
                }
                SchemaId::KeyGroupMembershipRequest => {
                    requests.push(MembershipRequestView::try_from(document.clone())?)
                }
                _ => (),
            }
            if document.id() == &key_group_id {
                key_group = Some(KeyGroupView::try_from(document.clone())?);
            }
        }
        if key_group.is_none() {
            return Err(KeyGroupError::InvalidMembership("this".to_string()));
        }

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
        KeyGroup::new_from_members(&key_group.unwrap(), &members, member_key_groups)
    }

    /// Create a key group from a key group view and a set of memberships.
    pub fn new_from_members(
        key_group: &KeyGroupView,
        members: &[Membership],
        member_key_groups: &[KeyGroup],
    ) -> Result<KeyGroup, KeyGroupError> {
        let mut mem = vec![];
        for membership in members {
            match membership.member() {
                Owner::Author(value) => {
                    // member_map.insert(value.clone(), membership.clone())
                    mem.push((value, membership));
                }
                Owner::KeyGroup(value) => {
                    match member_key_groups
                        .iter()
                        .find(|key_group| key_group.id() == value)
                    {
                        Some(key_group) => {
                            for (author, membership) in key_group.members() {
                                // member_map.insert(author.clone(), membership.clone())
                                mem.push((author, membership));
                            }
                        }
                        None => {
                            return Err(KeyGroupError::InvalidMembership("oops".to_string()));
                        }
                    };
                }
            };
        }

        let mut member_map: HashMap<Author, Membership> = HashMap::new();
        for (author, membership) in mem {
            if let Some(value) = member_map.get(author) {
                if value.accepted() {
                    continue;
                }
            }
            member_map.insert(author.clone(), membership.clone());
        }

        let key_group = KeyGroup {
            id: key_group.id().clone(),
            name: key_group.name().to_string(),
            members: member_map,
        };

        key_group.validate()?;
        Ok(key_group)
    }

    /// Returns the key group's id.
    pub fn id(&self) -> &DocumentId {
        &self.id
    }

    /// Access the key group's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Access the key group's members.
    pub fn members(&self) -> &HashMap<Author, Membership> {
        &self.members
    }

    /// Test whether an [`Author`] is a member.
    pub fn is_member(&self, author: &Author) -> bool {
        match self.members.get(author) {
            Some(membership) => membership.accepted(),
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
    id: DocumentId,
    view_id: DocumentViewId,
    name: String,
}

#[allow(dead_code)]
impl KeyGroupView {
    pub fn id(&self) -> &DocumentId {
        &self.id
    }

    /// The id of this key group view.
    pub fn view_id(&self) -> &DocumentViewId {
        &self.view_id
    }

    /// The name of this key group.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl TryFrom<Document> for KeyGroupView {
    type Error = SystemSchemaError;

    fn try_from(document: Document) -> Result<Self, Self::Error> {
        let name = match document.view().get("name") {
            Some(OperationValue::Text(value)) => Ok(value),
            Some(op) => Err(SystemSchemaError::InvalidField(
                "name".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("name".to_string())),
        }?;

        Ok(Self {
            id: document.id().clone(),
            view_id: document.view().id().clone(),
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
    use crate::operation::{OperationValue, PinnedRelation, Relation};
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
        let view = KeyGroupView::try_from(document).unwrap();
        let members: Vec<Membership> = vec![];
        assert_eq!(
            format!(
                "{}",
                KeyGroup::new_from_members(&view, &members, &[]).unwrap_err()
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
                fields(vec![(
                    "name",
                    OperationValue::Text("Strawberry Picking Gang".to_string()),
                )]),
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

        let frog_request = node.get_document(&frog_request_doc_id);

        let (frog_membership_doc_id, _) = send_to_node(
            &mut node,
            &frog,
            &create_operation(
                "key_group_membership_v1".parse().unwrap(),
                fields(vec![
                    (
                        "request",
                        OperationValue::PinnedRelation(PinnedRelation::new(DocumentViewId::from(
                            frog_request_doc_id,
                        ))),
                    ),
                    ("accepted", OperationValue::Boolean(true)),
                ]),
            ),
        )
        .unwrap();
        let frog_response = node.get_document(&frog_membership_doc_id);

        let key_group = KeyGroup::new(
            key_group_id.clone().into(),
            &[
                node.get_document(&key_group_id),
                frog_request.clone(),
                frog_response.clone(),
            ],
            &[],
        )
        .unwrap();

        assert!(key_group.is_member(&frog_author));
        let expected_key_group_id = key_group_id.as_str().parse::<DocumentId>().unwrap();
        assert_eq!(key_group.id(), &expected_key_group_id);

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
        let rabbit_request = node.get_document(&rabbit_request_doc_id);

        // But rabbit is not a member yet
        let key_group = KeyGroup::new(
            key_group_id.clone().into(),
            &[
                node.get_document(&key_group_id),
                frog_request.clone(),
                frog_response.clone(),
                rabbit_request.clone(),
            ],
            &[],
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
            key_group_id.clone().into(),
            &[
                node.get_document(&key_group_id),
                frog_request.clone(),
                frog_response,
                rabbit_request.clone(),
                rabbit_response.clone(),
            ],
            &[],
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
            key_group_id.clone().into(),
            &[
                node.get_document(&key_group_id),
                frog_request,
                frog_response,
                rabbit_request,
                rabbit_response,
            ],
            &[],
        )
        .unwrap();

        assert!(!key_group.is_member(&frog_author));
    }
}
