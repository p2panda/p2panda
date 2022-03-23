// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use crate::document::{DocumentId, Document, DocumentViewId};
use crate::identity::Author;
use crate::operation::OperationValue;
use crate::schema::key_group::Owner;
use crate::schema::system::SystemSchemaError;

#[derive(Clone, Debug)]
/// A request for key group membership.
pub struct MembershipRequestView {
    /// Identifies this request.
    id: DocumentViewId,

    /// The key group to be joined.
    key_group: DocumentId,

    /// Specifies whether membership is requested for a key group or the author of this document.
    member: Owner,
}

#[allow(dead_code)]
impl MembershipRequestView {
    pub fn new(author: &Author, membership_request: &Document) -> Result<Self, SystemSchemaError> {
        let key_group = match membership_request.view().get("key_group") {
            Some(OperationValue::Relation(value)) => Ok(value.document_id()),
            Some(op) => Err(SystemSchemaError::InvalidField(
                "key_group".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("key_group".to_string())),
        }?;

        let member = match membership_request.view().get("member") {
            Some(OperationValue::Relation(value)) => {
                Ok(Owner::KeyGroup(value.document_id().clone()))
            }
            Some(op) => Err(SystemSchemaError::InvalidField(
                "member".to_string(),
                op.to_owned(),
            )),
            None => Ok(Owner::Author(author.clone())),
        }?;

        Ok(MembershipRequestView {
            id: membership_request.view().id().clone(),
            key_group: key_group.clone(),
            member,
        })
    }

    /// The id of this membership request view.
    pub fn view_id(&self) -> &DocumentViewId {
        &self.id
    }

    /// The key group of this membership request view.
    pub fn key_group(&self) -> &DocumentId {
        &self.key_group
    }

    /// The public key or key group that is requesting membership.
    pub fn member(&self) -> &Owner {
        &self.member
    }
}

impl TryFrom<Document> for MembershipRequestView {
    type Error = SystemSchemaError;

    fn try_from(document: Document) -> Result<MembershipRequestView, Self::Error> {
        MembershipRequestView::new(document.author(), &document)
    }
}

#[cfg(test)]
mod test {
    use rstest::rstest;

    use crate::identity::KeyPair;
    use crate::operation::{OperationId, Relation};
    use crate::test_utils::fixtures::{random_key_pair, random_operation_id, fields};
    use crate::test_utils::mocks::{Client, send_to_node, Node};
    use crate::test_utils::utils::create_operation;

    use super::*;

    #[rstest]
    fn from_document(
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

        let document = node.get_document(&rabbit_request_hash);
        let author = Author::new(&rabbit.public_key()).unwrap();
        assert!(MembershipRequestView::new(&author, &document).is_ok());
    }
}
