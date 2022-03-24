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
#[derive(Debug, Clone)]
pub struct KeyGroup {
    id: DocumentId,
    name: String,
    members: HashMap<Author, Membership>,
}

impl KeyGroup {
    /// Create a key group from documents of itself, its memberships and membership requests.
    pub fn new_from_documents(
        key_group_id: DocumentId,
        documents: &[Document],
        member_key_groups: &[KeyGroup],
    ) -> Result<KeyGroup, KeyGroupError> {
        let mut key_group = None;
        let mut requests = HashMap::new();
        let mut responses = Vec::new();

        for document in documents {
            match document.schema() {
                SchemaId::KeyGroupMembership => {
                    responses.push(MembershipView::try_from(document.clone())?);
                }
                SchemaId::KeyGroupMembershipRequest => {
                    let request = MembershipRequestView::try_from(document.clone())?;
                    requests.insert(request.view_id().clone(), request);
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
            match requests.get(response.request()) {
                Some(request) => {
                    members.push(Membership::new(request.clone(), response.clone())?);
                }
                None => {
                    continue;
                }
            };
        }
        KeyGroup::new(&key_group.unwrap(), &members, member_key_groups)
    }

    /// Create a key group from a key group view and a set of memberships.
    pub fn new(
        key_group: &KeyGroupView,
        members: &[Membership],
        member_key_groups: &[KeyGroup],
    ) -> Result<KeyGroup, KeyGroupError> {
        // Collect all (author, membership) pairs from `members` parameter, including duplicate
        // author values.
        let mut member_pool = vec![];
        for membership in members {
            match membership.member() {
                // Simple case: for single key memberships just add that key to the pool.
                Owner::Author(value) => {
                    member_pool.push((value, membership));
                }
                // When a key group is a member, recursively add those key group's members to the
                // pool, assigned to a shared `membership`
                Owner::KeyGroup(value) => {
                    match member_key_groups
                        .iter()
                        .find(|key_group| key_group.id() == value)
                    {
                        Some(sub_key_group) => {
                            for (author, sub_membership) in sub_key_group.members() {
                                if sub_membership.accepted() {
                                    member_pool.push((author, membership));
                                }
                            }
                        }
                        None => {
                            return Err(KeyGroupError::MissingMemberKeyGroup(format!(
                                "{:?}",
                                value
                            )));
                        }
                    };
                }
            };
        }

        // Deduplicate so we have one membership per public key. A membership with more rights
        // takes precedence here. At the moment that is just memberships that are accepted, vs not
        // accepted.
        let mut member_map: HashMap<Author, Membership> = HashMap::new();
        for (author, membership) in member_pool {
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
    type Error = KeyGroupError;

    fn try_from(document: Document) -> Result<Self, Self::Error> {
        let name = match document.view().get("name") {
            Some(OperationValue::Text(value)) => Ok(value),
            Some(op) => Err(SystemSchemaError::InvalidField(
                "name".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("name".to_string())),
        }?;

        let view = Self {
            id: document.id().clone(),
            view_id: document.view().id().clone(),
            name: name.clone(),
        };
        view.validate()?;
        Ok(view)
    }
}

impl Validate for KeyGroupView {
    type Error = KeyGroupError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.name.is_empty() {
            return Err(KeyGroupError::InvalidName(self.name.clone()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use rstest::rstest;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::identity::{Author, KeyPair};
    use crate::operation::{OperationValue, PinnedRelation, Relation};
    use crate::schema::key_group::{Membership, Owner};
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
            format!("{}", KeyGroup::new(&view, &members, &[]).unwrap_err()),
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

        let key_group =
            KeyGroup::new_from_documents(key_group_id.clone().into(), &node.get_documents(), &[])
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
        let key_group =
            KeyGroup::new_from_documents(key_group_id.clone().into(), &node.get_documents(), &[])
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

        let key_group =
            KeyGroup::new_from_documents(key_group_id.clone().into(), &node.get_documents(), &[])
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

        let key_group =
            KeyGroup::new_from_documents(key_group_id.clone().into(), &node.get_documents(), &[])
                .unwrap();

        assert!(!key_group.is_member(&frog_author));

        // So Frog makes a new key group for blueberry picking that has the whole strawberry
        // picking gang in it.

        let (blueberry_id, _) = send_to_node(
            &mut node,
            &frog,
            &create_operation(
                "key_group_v1".parse().unwrap(),
                fields(vec![(
                    "name",
                    OperationValue::Text("Blueberry Picking Gang".to_string()),
                )]),
            ),
        )
        .unwrap();

        let (frog_blueberry_request_doc_id, _) = send_to_node(
            &mut node,
            &frog,
            &create_operation(
                "key_group_membership_request_v1".parse().unwrap(),
                fields(vec![(
                    "key_group",
                    OperationValue::Relation(Relation::new(DocumentId::new(
                        blueberry_id.clone().into(),
                    ))),
                )]),
            ),
        )
        .unwrap();

        let frog_blueberry_request = node.get_document(&frog_blueberry_request_doc_id);

        let (frog_blueberry_membership_doc_id, _) = send_to_node(
            &mut node,
            &frog,
            &create_operation(
                "key_group_membership_v1".parse().unwrap(),
                fields(vec![
                    (
                        "request",
                        OperationValue::PinnedRelation(PinnedRelation::new(DocumentViewId::from(
                            frog_blueberry_request_doc_id,
                        ))),
                    ),
                    ("accepted", OperationValue::Boolean(true)),
                ]),
            ),
        )
        .unwrap();

        let frog_blueberry_response = node.get_document(&frog_blueberry_membership_doc_id);

        // Rabbit concedes and asks for the whole strawberry picking gang to become members
        let (spg_blueberry_request_doc_id, _) = send_to_node(
            &mut node,
            &rabbit,
            &create_operation(
                "key_group_membership_request_v1".parse().unwrap(),
                fields(vec![
                    (
                        "key_group",
                        OperationValue::Relation(Relation::new(key_group.id().clone())),
                    ),
                    (
                        "member",
                        OperationValue::Owner(Relation::new(key_group.id().clone())),
                    ),
                ]),
            ),
        )
        .unwrap();

        let spg_blueberry_request = node.get_document(&spg_blueberry_request_doc_id);

        let (spg_blueberry_response_doc_id, _) = send_to_node(
            &mut node,
            &frog,
            &create_operation(
                "key_group_membership_v1".parse().unwrap(),
                fields(vec![
                    (
                        "request",
                        OperationValue::PinnedRelation(PinnedRelation::new(DocumentViewId::from(
                            spg_blueberry_request_doc_id,
                        ))),
                    ),
                    ("accepted", OperationValue::Boolean(true)),
                ]),
            ),
        )
        .unwrap();
        let spg_blueberry_response = node.get_document(&spg_blueberry_response_doc_id);

        let blueberry_picking_gang = KeyGroup::new_from_documents(
            blueberry_id.into(),
            &node.get_documents(),
            &[key_group.clone()],
        )
        .unwrap();

        // Rabbit is a member by way of the Strawberry Picking Gang
        assert_eq!(
            blueberry_picking_gang.get(&rabbit_author).unwrap().member(),
            &Owner::KeyGroup(key_group.id().clone()),
            "{:?}",
            blueberry_picking_gang.get(&rabbit_author)
        );

        // Frog is not a member as part of the Strawberry Picking Gang because she added herself
        // directly to the group and her membership in the SPG is void.
        assert_eq!(
            blueberry_picking_gang.get(&frog_author).unwrap().member(),
            &Owner::Author(frog_author.clone()),
            "{:?}",
            blueberry_picking_gang.get(&frog_author)
        );
    }
}
