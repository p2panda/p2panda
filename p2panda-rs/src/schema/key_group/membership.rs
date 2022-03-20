// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use crate::document::{DocumentView, DocumentViewId};
use crate::operation::OperationValue;
use crate::schema::system::SystemSchemaError;

use super::error::KeyGroupError;
use super::membership_request::MembershipRequestView;
use super::Owner;

#[derive(Clone, Debug)]
pub struct Membership {
    view_id: DocumentViewId,
    member: Owner,
    accepted: bool,
}

impl Membership {
    pub fn new(
        request: MembershipRequestView,
        response: MembershipView,
    ) -> Result<Self, KeyGroupError> {
        if response.request() != request.view_id() {
            return Err(KeyGroupError::InvalidMembership(
                "response doesn't reference supplied request".to_string(),
            ));
        }

        Ok(Membership {
            view_id: response.view_id,
            member: request.member().clone(),
            accepted: response.accepted,
        })
    }

    pub fn view_id(&self) -> &DocumentViewId {
        &self.view_id
    }

    pub fn member(&self) -> &Owner {
        &self.member
    }

    pub fn accepted(&self) -> &bool {
        &self.accepted
    }
}

#[derive(Clone, Debug)]
pub struct MembershipView {
    view_id: DocumentViewId,
    request: DocumentViewId,
    accepted: bool,
}

#[allow(dead_code)]
impl MembershipView {
    /// The id of this membership request view.
    pub fn view_id(&self) -> &DocumentViewId {
        &self.view_id
    }

    /// The view id of the request for this membership.
    pub fn request(&self) -> &DocumentViewId {
        &self.request
    }
}

impl TryFrom<DocumentView> for MembershipView {
    type Error = SystemSchemaError;

    fn try_from(document_view: DocumentView) -> Result<Self, Self::Error> {
        // @TODO: validate that document view has the right schema
        let request = match document_view.get("request") {
            Some(OperationValue::PinnedRelation(value)) => Ok(value.view_id()),
            Some(op) => Err(SystemSchemaError::InvalidField(
                "request".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("request".to_string())),
        }?;

        let accepted = match document_view.get("accepted") {
            Some(OperationValue::Boolean(value)) => Ok(value),
            Some(op) => Err(SystemSchemaError::InvalidField(
                "accepted".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("accepted".to_string())),
        }?;

        Ok(MembershipView {
            view_id: document_view.id().clone(),
            request,
            accepted: accepted.to_owned(),
        })
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use rstest::rstest;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::identity::{Author, KeyPair};
    use crate::operation::{OperationId, OperationValue, PinnedRelation, Relation};
    use crate::schema::key_group::{MembershipView, MembershipRequestView};
    use crate::test_utils::fixtures::{
        create_operation, fields, random_key_pair, random_operation_id,
    };
    use crate::test_utils::mocks::{send_to_node, Client, Node};

    use super::Membership;

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
                        OperationValue::PinnedRelation(PinnedRelation::new(DocumentViewId::from(request_id))),
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
