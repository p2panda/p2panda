// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use crate::Validate;
use crate::document::{Document, DocumentId, DocumentViewId};
use crate::operation::OperationValue;
use crate::schema::key_group::Owner;
use crate::schema::system::SystemSchemaError;

#[derive(Clone, Debug)]
/// A request for key group membership.
pub struct MembershipRequestView(Document);

#[allow(dead_code)]
impl MembershipRequestView {
    /// Create a membership request view from its author and the request's document.
    pub fn new(membership_request: &Document) -> Result<Self, SystemSchemaError> {
        let doc = Self(membership_request.clone());
        doc.validate()?;
        Ok(doc)
    }

    /// The id of this membership request view.
    pub fn view_id(&self) -> &DocumentViewId {
        self.0.view().id()
    }

    /// The key group of this membership request view.
    pub fn key_group(&self) -> &DocumentId {
        match self.0.view().get("key_group") {
            Some(OperationValue::Relation(relation)) => relation.document_id(),
            _ => panic!()
        }
    }

    /// The public key or key group that is requesting membership.
    pub fn member(&self) -> Owner {
        match self.0.view().get("owner") {
            Some(OperationValue::Owner(relation)) => Owner::KeyGroup(relation.document_id().clone()),
            Some(_) => panic!(),
            None => Owner::Author(self.0.author().clone())
        }
    }
}

impl Validate for MembershipRequestView {
    type Error = SystemSchemaError;

    fn validate(&self) -> Result<(), Self::Error> {
        match self.0.view().get("key_group") {
            Some(OperationValue::Relation(_)) => Ok(()),
            Some(op) => Err(SystemSchemaError::InvalidField(
                "key_group".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("key_group".to_string())),
        }?;

        match self.0.view().get("member") {
            Some(OperationValue::Owner(_)) => Ok(()),
            Some(op) => Err(SystemSchemaError::InvalidField(
                "member".to_string(),
                op.to_owned(),
            )),
            None => Ok(()),
        }?;

        Ok(())
    }
}

impl TryFrom<Document> for MembershipRequestView {
    type Error = SystemSchemaError;

    fn try_from(document: Document) -> Result<MembershipRequestView, Self::Error> {
        MembershipRequestView::new(&document)
    }
}

#[cfg(test)]
mod test {
    use rstest::rstest;

    use crate::identity::KeyPair;
    use crate::operation::{OperationId, Relation};
    use crate::test_utils::fixtures::{fields, random_key_pair, random_operation_id};
    use crate::test_utils::mocks::{send_to_node, Client, Node};
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
        assert!(MembershipRequestView::new(&document).is_ok());
    }
}
