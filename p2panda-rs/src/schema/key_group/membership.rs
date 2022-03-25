// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use crate::document::{Document, DocumentViewId};
use crate::operation::OperationValue;
use crate::schema::system::SystemSchemaError;
use crate::schema::SchemaId;
use crate::Validate;

use super::error::KeyGroupError;
use super::membership_request::MembershipRequestView;
use super::Owner;

/// Memership in a key group.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Membership {
    response_view: Option<DocumentViewId>,
    request_view: DocumentViewId,
    member: Owner,
    accepted: bool,
}

impl Membership {
    /// Create a new membership instance.
    ///
    /// Requires matching a membership request that matches the membership response's request field.
    pub fn new(
        request: MembershipRequestView,
        response: Option<MembershipView>,
    ) -> Result<Self, KeyGroupError> {
        match response {
            Some(response) => {
                if response.request() != request.view_id() {
                    return Err(KeyGroupError::InvalidMembership(
                        "response doesn't reference supplied request".to_string(),
                    ));
                }
                Ok(Membership {
                    request_view: request.view_id().clone(),
                    response_view: Some(response.view_id().clone()),
                    member: request.member(),
                    accepted: *response.accepted(),
                })
            }
            None => Ok(Membership {
                request_view: request.view_id().clone(),
                response_view: None,
                member: request.member(),
                accepted: false,
            }),
        }
    }

    /// Access the membership's view id.
    pub fn response_view_id(&self) -> &Option<DocumentViewId> {
        &self.response_view
    }

    /// Access the [`Owner`] whose membership this describes.
    pub fn member(&self) -> &Owner {
        &self.member
    }

    /// Returns true if this membership is accepted.
    ///
    /// Memberships that are not accepted have been revoked and should be considered void.
    pub fn accepted(&self) -> bool {
        self.accepted
    }
}

/// Represents a membership document.
#[derive(Clone, Debug)]
pub struct MembershipView(Document);

#[allow(dead_code)]
impl MembershipView {
    /// The id of this membership request view.
    pub fn view_id(&self) -> &DocumentViewId {
        self.0.view().id()
    }

    /// The view id of the request for this membership.
    pub fn request(&self) -> &DocumentViewId {
        match self.0.view().get("request") {
            Some(OperationValue::PinnedRelation(value)) => value.view_id(),
            _ => panic!(),
        }
    }

    /// Returns true if this membership is accepted.
    pub fn accepted(&self) -> &bool {
        match self.0.view().get("accepted") {
            Some(OperationValue::Boolean(value)) => value,
            _ => panic!(),
        }
    }
}

impl Validate for MembershipView {
    type Error = SystemSchemaError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.0.schema() != &SchemaId::KeyGroupMembership {
            return Err(SystemSchemaError::UnexpectedSchema(
                SchemaId::KeyGroupMembership,
                self.0.schema().clone(),
            ));
        }

        match self.0.view().get("request") {
            Some(OperationValue::PinnedRelation(_)) => Ok(()),
            Some(op) => Err(SystemSchemaError::InvalidField(
                "request".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("request".to_string())),
        }?;

        match self.0.view().get("accepted") {
            Some(OperationValue::Boolean(_)) => Ok(()),
            Some(op) => Err(SystemSchemaError::InvalidField(
                "accepted".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("accepted".to_string())),
        }?;
        Ok(())
    }
}

impl TryFrom<Document> for MembershipView {
    type Error = SystemSchemaError;

    fn try_from(document: Document) -> Result<Self, Self::Error> {
        let membership_view = Self(document);
        membership_view.validate()?;
        Ok(membership_view)
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use rstest::rstest;

    use crate::document::DocumentViewId;
    use crate::identity::KeyPair;
    use crate::operation::{OperationId, OperationValue, PinnedRelation};
    use crate::schema::key_group::MembershipView;
    use crate::test_utils::fixtures::{
        create_operation, fields, random_key_pair, random_operation_id,
    };
    use crate::test_utils::mocks::{send_to_node, Client, Node};

    #[rstest]
    fn from_document(
        random_key_pair: KeyPair,
        #[from(random_operation_id)] request_id: OperationId,
    ) {
        let frog = Client::new("frog".to_string(), random_key_pair);
        let mut node = Node::new();

        let (membership_doc_id, _) = send_to_node(
            &mut node,
            &frog,
            &create_operation(
                "key_group_membership_v1".parse().unwrap(),
                fields(vec![
                    (
                        "request",
                        OperationValue::PinnedRelation(PinnedRelation::new(DocumentViewId::from(
                            request_id,
                        ))),
                    ),
                    ("accepted", OperationValue::Boolean(true)),
                ]),
            ),
        )
        .unwrap();

        let document_view = node.get_document(&membership_doc_id);
        MembershipView::try_from(document_view).unwrap();
    }
}
