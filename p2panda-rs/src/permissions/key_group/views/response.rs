// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use crate::document::{Document, DocumentViewId};
use crate::operation::OperationValue;
use crate::schema::system::SystemSchemaError;
use crate::schema::SchemaId;
use crate::Validate;

/// Represents a membership document.
#[derive(Clone, Debug)]
pub struct MembershipResponseView(Document);

#[allow(dead_code)]
impl MembershipResponseView {
    /// The id of this membership request view.
    pub fn view_id(&self) -> &DocumentViewId {
        self.0.view().id()
    }

    /// The view id of the request for this membership.
    pub fn request(&self) -> &DocumentViewId {
        match self.0.view().get("request") {
            Some(OperationValue::PinnedRelation(value)) => value.view_id(),
            // This code is unreachable as a `MembershipResponseView` can only be created via
            // its `TryFrom<Document>` impl, which checks that this field exists.
            _ => panic!(),
        }
    }

    /// Returns true if this membership is accepted.
    pub fn accepted(&self) -> &bool {
        match self.0.view().get("accepted") {
            Some(OperationValue::Boolean(value)) => value,
            // This code is unreachable as a `MembershipResponseView` can only be created via
            // its `TryFrom<Document>` impl, which checks that this field exists.
            _ => panic!(),
        }
    }
}

impl Validate for MembershipResponseView {
    type Error = SystemSchemaError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.0.is_deleted() {
            return Err(SystemSchemaError::Deleted(self.0.id().clone()));
        }

        if self.0.schema() != &SchemaId::KeyGroupResponse {
            return Err(SystemSchemaError::UnexpectedSchema(
                SchemaId::KeyGroupResponse,
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

impl TryFrom<Document> for MembershipResponseView {
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
    use crate::permissions::key_group::MembershipResponseView;
    use crate::schema::SchemaId;
    use crate::test_utils::constants::DEFAULT_HASH;
    use crate::test_utils::fixtures::{
        create_operation, document, document_id, document_view_id, fields, key_pair,
    };

    #[rstest]
    // Correct fields defined - this should pass
    #[case(
        ("request", OperationValue::PinnedRelation(document_view_id(vec![DEFAULT_HASH]).into())),
        ("accepted", OperationValue::Boolean(true)),
        None
    )]
    // `request` field with wrong value type
    #[case(
        ("request", OperationValue::Relation(document_id(DEFAULT_HASH).into())),
        ("accepted", OperationValue::Boolean(true)),
        Some("invalid field 'request' with value Relation(Relation(DocumentId(OperationId(Hash(\"0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543\")))))")
    )]
    // `request` field missing
    #[case(
        ("requesd", OperationValue::PinnedRelation(document_view_id(vec![DEFAULT_HASH]).into())),
        ("accepted", OperationValue::Boolean(true)),
        Some("missing field 'request'")
    )]
    // `accepted` field with wrong value type
    #[case(
        ("request", OperationValue::PinnedRelation(document_view_id(vec![DEFAULT_HASH]).into())),
        ("accepted", OperationValue::Text("yes".to_string())),
        Some("invalid field 'accepted' with value Text(\"yes\")")
    )]
    // `accepted` field missing
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
                SchemaId::KeyGroupResponse,
                fields(vec![request_field, accepted_field]),
            ),
            key_pair,
            false,
        );
        let result = MembershipResponseView::try_from(doc);
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
                SchemaId::KeyGroupResponse,
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
        let result = MembershipResponseView::try_from(doc);
        assert_eq!(
            format!("{}", result.unwrap_err()),
            "unable to create view for deleted document DocumentId(Oper\
                ationId(Hash(\"00203d3e3644544c511a7e3f14d76cac93b58ba8\
                7249e27f891ee4d605c03489d67f\")))"
        )
    }

    #[rstest]
    fn wrong_schema(document: Document) {
        let result = MembershipResponseView::try_from(document);
        assert_eq!(
            format!("{}", result.unwrap_err()),
            "expected schema KeyGroupResponse got Application(\"venue\", DocumentViewId([Operati\
                onId(Hash(\"0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\"\
            ))]))"
        )
    }
}
