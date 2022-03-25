// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::convert::TryFrom;

use log::debug;

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
    view_id: DocumentViewId,
    name: String,
    members: HashMap<Author, Vec<Membership>>,
}

impl KeyGroup {
    /// Create a key group from a key group view and a set of memberships.
    ///
    /// The members parameter must only contain memberships of the key group to be created.
    /// The `member_key_groups` parameter must contain all key groups that have memberships and may
    /// contain additional unrelated key groups.
    pub fn new(
        key_group: &KeyGroupView,
        members: &[Membership],
        member_key_groups: &[KeyGroup],
    ) -> Result<KeyGroup, KeyGroupError> {
        debug!("Building {}", key_group.name());

        // Collect all (author, membership) pairs from `members` parameter, including duplicate
        // author values.
        let mut member_pool = vec![];
        for membership in members {
            match membership.member() {
                // Simple case: for single key memberships just add that key to the pool.
                Owner::Author(value) => {
                    debug!("Adding author {:?}", value.as_str());
                    member_pool.push((value, membership));
                }

                // When a key group is a member, recursively add that key group's members to the
                // pool, assigned to a shared `membership`
                Owner::KeyGroup(sub_key_group_id) => {
                    let sub_key_group = member_key_groups
                        .iter()
                        .find(|key_group| key_group.id() == sub_key_group_id)
                        .ok_or_else(|| {
                            KeyGroupError::MissingMemberKeyGroup(format!("{:?}", sub_key_group_id))
                        })?;

                    debug!("Adding members of key group {}", sub_key_group.name());
                    for author in sub_key_group.members().keys() {
                        // Only add if a member is accepted within the sub key group.
                        if sub_key_group.is_member(author) {
                            debug!("Adding {}", author.as_str());
                            member_pool.push((author, membership));
                        } else {
                            debug!("Skipping {}", author.as_str());
                        }
                    }
                }
            };
        }

        // Deduplicate so we have one membership per public key. A membership with more rights
        // takes precedence here. At the moment that is just memberships that are accepted, vs not
        // accepted.
        let mut member_map: HashMap<Author, Vec<Membership>> = HashMap::new();
        for (author, membership) in member_pool {
            if let Some(previous_memberships) = member_map.get_mut(author) {
                previous_memberships.push(membership.clone());
            } else {
                member_map.insert(author.clone(), vec![membership.clone()]);
            }
        }

        let key_group = KeyGroup {
            id: key_group.id().clone(),
            view_id: key_group.view_id().clone(),
            name: key_group.name().to_string(),
            members: member_map,
        };

        Ok(key_group)
    }

    /// Create a new key group from predecessor documents.
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
                members.push(Membership::from_confirmation(
                    request.clone(),
                    Some(response.clone()),
                )?);
            };
        }

        for request in requests.values() {
            members.push(Membership::from_confirmation(request.clone(), None)?);
        }

        KeyGroup::new(&key_group.unwrap(), &members, member_key_groups)
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
    pub fn members(&self) -> &HashMap<Author, Vec<Membership>> {
        &self.members
    }

    /// Test whether an [`Author`] is a member.
    pub fn is_member(&self, author: &Author) -> bool {
        match self.members.get(author) {
            Some(memberships) => memberships.iter().any(|membership| membership.accepted()),
            None => false,
        }
    }

    /// Get the membership for an [`Author`].
    pub fn get(&self, author: &Author) -> Option<&Vec<Membership>> {
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
    pub fn request_membership(&self, key_group_id: Option<&DocumentId>) -> Operation {
        let mut request_fields = OperationFields::new();
        request_fields
            .add(
                "key_group",
                OperationValue::Relation(Relation::new(self.id().clone())),
            )
            .unwrap();
        if let Some(key_group_id) = key_group_id {
            request_fields
                .add(
                    "member",
                    OperationValue::Owner(Relation::new(key_group_id.clone())),
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
pub struct KeyGroupView(Document);

#[allow(dead_code)]
impl KeyGroupView {
    pub fn id(&self) -> &DocumentId {
        self.0.id()
    }

    /// The id of this key group view.
    pub fn view_id(&self) -> &DocumentViewId {
        self.0.view().id()
    }

    /// The name of this key group.
    pub fn name(&self) -> &str {
        match self.0.view().get("name") {
            Some(OperationValue::Text(value)) => value,
            _ => panic!(),
        }
    }
}

impl TryFrom<Document> for KeyGroupView {
    type Error = KeyGroupError;

    fn try_from(document: Document) -> Result<Self, Self::Error> {
        let view = Self(document);
        view.validate()?;
        Ok(view)
    }
}

impl Validate for KeyGroupView {
    type Error = SystemSchemaError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.0.is_deleted() {
            return Err(SystemSchemaError::Deleted(format!("{:?}", self.0)));
        }

        let name = match self.0.view().get("name") {
            Some(OperationValue::Text(value)) => Ok(value),
            Some(op) => Err(SystemSchemaError::InvalidField(
                "name".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("name".to_string())),
        }?;

        if name.is_empty() {
            return Err(SystemSchemaError::InvalidField(
                "name".to_string(),
                self.0.view().get("name").unwrap().clone(),
            ));
        }

        Ok(())
    }
}
