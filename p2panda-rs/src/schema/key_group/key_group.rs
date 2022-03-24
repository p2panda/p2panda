// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::convert::TryFrom;

use crate::document::{Document, DocumentId, DocumentViewId};
use crate::identity::Author;
use crate::operation::{Operation, OperationFields, OperationValue, PinnedRelation, Relation};
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
    /// Create a new key group from predecessor items.
    ///
    /// The `documents` parameter should contain documents for the key group itself and all
    /// (request, response) pairs.
    ///
    /// The `member_key_groups` parameter should contain all [`KeyGroup`]s that are members.
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
                    if request.key_group() == &key_group_id {
                        requests.insert(request.view_id().clone(), request);
                    }
                }
                _ => (),
            }
            if document.id() == &key_group_id {
                key_group = Some(KeyGroupView::try_from(document.clone())?);
            }
        }
        if key_group.is_none() {
            return Err(KeyGroupError::MissingKeyGroupView);
        }

        let mut members: Vec<Membership> = Vec::new();
        for response in responses {
            // Remove requests for which we have a response
            if let Some(request) = requests.remove(response.request()) {
                members.push(Membership::new(request.clone(), Some(response.clone()))?);
            };
        }

        for request in requests.values() {
            members.push(Membership::new(request.clone(), None)?);
        }

        KeyGroup::new(&key_group.unwrap(), &members, member_key_groups)
    }

    /// Create a key group from a key group view and a set of memberships.
    ///
    /// The members parameter must only contain memberships of the key group to be created.
    /// The `member_key_groups` parameter must contain all key groups that have membership and may
    /// contain additional unrelated key groups.
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
                // When a key group is a member, recursively add that key group's members to the
                // pool, assigned to a shared `membership`
                Owner::KeyGroup(value) => {
                    match member_key_groups
                        .iter()
                        .find(|key_group| key_group.id() == value)
                    {
                        Some(sub_key_group) => {
                            for (author, sub_membership) in sub_key_group.members() {
                                // Only add if a member is accepted within the sub key group.
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

    /// Make create operation for key group
    pub fn create(name: &str) -> Operation {
        let mut key_group_fields = OperationFields::new();
        key_group_fields
            .add("name", OperationValue::Text(name.to_string()))
            .unwrap();
        Operation::new_create(SchemaId::KeyGroup, key_group_fields).unwrap()
    }

    /// Make create operation for membership requests
    pub fn request_membership(&self, member: &Owner) -> Operation {
        let mut request_fields = OperationFields::new();
        request_fields
            .add(
                "key_group",
                OperationValue::Relation(Relation::new(self.id().clone())),
            )
            .unwrap();
        if let Owner::KeyGroup(kg_member) = member {
            request_fields
                .add(
                    "member",
                    OperationValue::Owner(Relation::new(kg_member.clone())),
                )
                .unwrap();
        }
        Operation::new_create(SchemaId::KeyGroupMembershipRequest, request_fields).unwrap()
    }

    /// Make a new response for a membership request.
    pub fn respond_to_request(request_view_id: &DocumentViewId, accepted: bool) -> Operation {
        let mut response_fields = OperationFields::new();
        response_fields
            .add("accepted", OperationValue::Boolean(accepted))
            .unwrap();
        response_fields
            .add(
                "request",
                OperationValue::PinnedRelation(PinnedRelation::new(request_view_id.clone())),
            )
            .unwrap();
        Operation::new_create(SchemaId::KeyGroupMembership, response_fields).unwrap()
    }

    /// Update a membership given a previous response's view id.
    pub fn update_membership(response_view_id: &DocumentViewId, accepted: bool) -> Operation {
        let mut response_fields = OperationFields::new();
        response_fields
            .add("accepted", OperationValue::Boolean(accepted))
            .unwrap();
        Operation::new_update(
            SchemaId::KeyGroupMembership,
            response_view_id.graph_tips().to_vec(),
            response_fields,
        )
        .unwrap()
    }
}

/// Represents a root key group definition.
///
/// Can be used to make a [`KeyGroup`].
#[derive(Clone, Debug)]
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
    use rstest::rstest;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::identity::{Author, KeyPair};

    use crate::schema::key_group::Owner;
    use crate::test_utils::fixtures::random_key_pair;
    use crate::test_utils::mocks::{send_to_node, Client, Node};

    use super::KeyGroup;

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
        let (create_hash, _) = send_to_node(
            &mut node,
            &frog,
            &KeyGroup::create("Strawberry Picking Gang"),
        )
        .unwrap();

        let key_group_id: DocumentId = create_hash.into();

        let key_group =
            KeyGroup::new_from_documents(key_group_id.clone(), &node.get_documents(), &[]).unwrap();

        // ... and makes herself a member
        let (frog_request_doc_id, _) = send_to_node(
            &mut node,
            &frog,
            &key_group.request_membership(&frog_author.clone().into()),
        )
        .unwrap();

        let (frog_membership_doc_id, _) = send_to_node(
            &mut node,
            &frog,
            &KeyGroup::respond_to_request(&DocumentViewId::from(frog_request_doc_id), true),
        )
        .unwrap();

        let key_group =
            KeyGroup::new_from_documents(key_group_id.clone(), &node.get_documents(), &[]).unwrap();

        assert!(key_group.is_member(&frog_author));
        let expected_key_group_id = key_group_id.as_str().parse::<DocumentId>().unwrap();
        assert_eq!(key_group.id(), &expected_key_group_id);

        // Rabbit asks to become a member as well
        let (rabbit_request_doc_id, _) = send_to_node(
            &mut node,
            &rabbit,
            &key_group.request_membership(&rabbit_author.clone().into()),
        )
        .unwrap();
        node.get_document(&rabbit_request_doc_id);

        // But rabbit is not a member yet
        let key_group =
            KeyGroup::new_from_documents(key_group_id.clone(), &node.get_documents(), &[]).unwrap();

        assert!(!key_group.is_member(&rabbit_author));

        // Now frog let's rabbit in :)
        send_to_node(
            &mut node,
            &frog,
            &KeyGroup::respond_to_request(&DocumentViewId::from(rabbit_request_doc_id), true),
        )
        .unwrap();

        let key_group =
            KeyGroup::new_from_documents(key_group_id.clone(), &node.get_documents(), &[]).unwrap();

        assert!(key_group.is_member(&rabbit_author));

        // But rabbit would rather pick strawberries alone.
        send_to_node(
            &mut node,
            &rabbit,
            &KeyGroup::update_membership(&DocumentViewId::from(frog_membership_doc_id), false),
        )
        .unwrap();

        let key_group =
            KeyGroup::new_from_documents(key_group_id.clone(), &node.get_documents(), &[]).unwrap();

        assert!(!key_group.is_member(&frog_author));

        // So Frog makes a new key group for blueberry picking that has the whole strawberry
        // picking gang in it.

        let (blueberry_id, _) = send_to_node(
            &mut node,
            &frog,
            &KeyGroup::create("Blueberry Picking Gang"),
        )
        .unwrap();

        let blueberry_picking_gang =
            KeyGroup::new_from_documents(blueberry_id.clone().into(), &node.get_documents(), &[])
                .unwrap();

        let (frog_blueberry_request_doc_id, _) = send_to_node(
            &mut node,
            &frog,
            &blueberry_picking_gang.request_membership(&frog_author.clone().into()),
        )
        .unwrap();

        send_to_node(
            &mut node,
            &frog,
            &KeyGroup::respond_to_request(
                &DocumentViewId::from(frog_blueberry_request_doc_id),
                true,
            ),
        )
        .unwrap();

        // Rabbit concedes and asks for the whole strawberry picking gang to also become members
        let (spg_blueberry_request_doc_id, _) = send_to_node(
            &mut node,
            &rabbit,
            &blueberry_picking_gang.request_membership(&key_group.clone().into()),
        )
        .unwrap();

        send_to_node(
            &mut node,
            &frog,
            &KeyGroup::respond_to_request(
                &DocumentViewId::from(spg_blueberry_request_doc_id),
                true,
            ),
        )
        .unwrap();

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
