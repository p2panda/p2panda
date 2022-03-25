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
    accepted: bool,
    has_response: bool,
    member: Owner,
}

impl Membership {
    /// Create a new membership instance.
    ///
    /// Use this constructor if you have validated the membership already.
    ///
    /// `member` is the identity whose membership is represented
    /// `response` encodes whether a valid response is known and whether that response was to
    ///   accept or reject the membership.
    pub fn new(member: &Owner, response: Option<bool>) -> Self {
        Self {
            accepted: response.is_some() && response.unwrap(),
            has_response: response.is_some(),
            member: member.clone(),
        }
    }

    /// Parse membership from a request and optional response.
    ///
    /// Request and response must match on the response's `request` field.
    pub fn from_confirmation(
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
                    accepted: *response.accepted(),
                    has_response: true,
                    member: request.member(),
                })
            }
            None => Ok(Membership {
                accepted: false,
                has_response: false,
                member: request.member(),
            }),
        }
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

    /// Returns true if this membership has a valid response.
    pub fn has_response(&self) -> bool {
        self.has_response
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
        if self.0.is_deleted() {
            return Err(SystemSchemaError::Deleted(self.0.id().clone()));
        }

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
    use std::convert::{TryFrom, TryInto};

    use rstest::rstest;

    use crate::document::{Document, DocumentId, DocumentViewId};
    use crate::identity::{Author, KeyPair};
    use crate::operation::OperationValue;
    use crate::schema::key_group::{KeyGroup, Membership, MembershipView};
    use crate::schema::SchemaId;
    use crate::test_utils::constants::DEFAULT_HASH;
    use crate::test_utils::fixtures::{
        create_operation, document, document_id, document_view_id, fields, key_pair,
        random_key_pair,
    };
    use crate::test_utils::mocks::{send_to_node, Client, Node};

    #[rstest]
    #[case(
        ("request", OperationValue::PinnedRelation(document_view_id(vec![DEFAULT_HASH]).into())),
        ("accepted", OperationValue::Boolean(true)),
        None
    )]
    #[case(
        ("request", OperationValue::Relation(document_id(DEFAULT_HASH).into())),
        ("accepted", OperationValue::Boolean(true)),
        Some("invalid field 'request' with value Relation(Relation(DocumentId(OperationId(Hash(\"0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543\")))))")
    )]
    #[case(
        ("requesd", OperationValue::PinnedRelation(document_view_id(vec![DEFAULT_HASH]).into())),
        ("accepted", OperationValue::Boolean(true)),
        Some("missing field 'request'")
    )]
    #[case(
        ("request", OperationValue::PinnedRelation(document_view_id(vec![DEFAULT_HASH]).into())),
        ("accepted", OperationValue::Text("yes".to_string())),
        Some("invalid field 'accepted' with value Text(\"yes\")")
    )]
    #[case(
        ("request", OperationValue::PinnedRelation(document_view_id(vec![DEFAULT_HASH]).into())),
        ("acceptet", OperationValue::Boolean(true)),
        Some("missing field 'accepted'")
    )]
    fn field_values(
        #[case] request_field: (&str, OperationValue),
        #[case] accepted_field: (&str, OperationValue),
        key_pair: KeyPair,
        #[case] expected_err: Option<&str>,
    ) {
        let doc = document(
            create_operation(
                SchemaId::KeyGroupMembership,
                fields(vec![request_field, accepted_field]),
            ),
            key_pair,
            false,
        );
        let result = MembershipView::try_from(doc);
        match expected_err {
            Some(err_str) => {
                assert_eq!(format!("{}", result.unwrap_err()), err_str)
            }
            None => assert!(result.is_ok(), "{:?}", result.unwrap_err()),
        };
    }

    #[rstest]
    fn deleted_doc(key_pair: KeyPair) {
        let doc = document(
            create_operation(
                SchemaId::KeyGroupMembership,
                fields(vec![
                    (
                        "request",
                        OperationValue::PinnedRelation(document_view_id(vec![DEFAULT_HASH]).into()),
                    ),
                    ("accepted", OperationValue::Boolean(true)),
                ]),
            ),
            key_pair,
            true,
        );
        let result = MembershipView::try_from(doc);
        assert_eq!(
            format!("{}", result.unwrap_err()),
            "unable to create view for deleted \
        document DocumentId(OperationId(Hash(\"0020b068fde5cb5a738ee3ef2f6f54663d5236095839c844917\
        22a2f6ca507118237\")))"
        )
    }

    #[rstest]
    fn wrong_schema(document: Document) {
        let result = MembershipView::try_from(document);
        assert_eq!(
            format!("{}", result.unwrap_err()),
            "expected schema KeyGroupMembership got Application(PinnedRelation(DocumentViewId([Ope\
                rationId(Hash(\"0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc7\
                8b\"))])))"
        )
    }

    #[rstest]
    fn parse_membership(#[from(random_key_pair)] frog_key_pair: KeyPair) {
        // Test setup
        let frog = Client::new("frog".to_string(), frog_key_pair);
        let frog_author = Author::new(&frog.public_key()).unwrap();

        let mut node = Node::new();

        // Frog creates the 'Strawberry Picking Gang' key group
        let (create_hash, _) = send_to_node(
            &mut node,
            &frog,
            &KeyGroup::create("Strawberry Picking Gang"),
        )
        .unwrap();

        let key_group =
            KeyGroup::new_from_documents(DocumentId::from(create_hash), &node.get_documents(), &[])
                .unwrap();

        // ... and requests membership
        let (frog_request_doc_id, _) =
            send_to_node(&mut node, &frog, &key_group.request_membership(None)).unwrap();

        // She should be an unconfirmed memberb
        let membership = Membership::from_confirmation(
            node.get_document(&frog_request_doc_id).try_into().unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(
            membership,
            Membership::new(&frog_author.clone().into(), None)
        );
        assert!(!membership.accepted());

        // She responds to the request
        let (frog_membership_doc_id, _) = send_to_node(
            &mut node,
            &frog,
            &KeyGroup::respond_to_request(&DocumentViewId::from(frog_request_doc_id.clone()), true),
        )
        .unwrap();

        // She should be a confirmed member
        let membership = Membership::from_confirmation(
            node.get_document(&frog_request_doc_id).try_into().unwrap(),
            Some(
                node.get_document(&frog_membership_doc_id)
                    .try_into()
                    .unwrap(),
            ),
        )
        .unwrap();
        assert_eq!(
            membership,
            Membership::new(&frog_author.clone().into(), Some(true))
        );
        assert!(membership.accepted());

        // She revokes her membership
        send_to_node(
            &mut node,
            &frog,
            &KeyGroup::update_membership(
                &DocumentViewId::from(frog_membership_doc_id.clone()),
                false,
            ),
        )
        .unwrap();

        // She should not be a member anymore
        let membership = Membership::from_confirmation(
            node.get_document(&frog_request_doc_id).try_into().unwrap(),
            Some(
                node.get_document(&frog_membership_doc_id)
                    .try_into()
                    .unwrap(),
            ),
        )
        .unwrap();
        assert_eq!(
            membership,
            Membership::new(&frog_author.into(), Some(false))
        );
        assert!(!membership.accepted());
    }
}
