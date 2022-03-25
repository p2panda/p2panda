// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use crate::document::{Document, DocumentViewId};
use crate::operation::OperationValue;
use crate::schema::system::SystemSchemaError;
use crate::schema::SchemaId;
use crate::Validate;

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
    use std::convert::TryFrom;

    use rstest::rstest;

    use crate::document::Document;
    use crate::identity::KeyPair;
    use crate::operation::OperationValue;
    use crate::schema::key_group::MembershipView;
    use crate::schema::SchemaId;
    use crate::test_utils::constants::DEFAULT_HASH;
    use crate::test_utils::fixtures::{
        create_operation, document, document_id, document_view_id, fields, key_pair,
    };

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
}
