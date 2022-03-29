// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use crate::document::{Document, DocumentId, DocumentViewId};
use crate::operation::OperationValue;
use crate::permissions::key_group::Owner;
use crate::schema::system::SystemSchemaError;
use crate::Validate;

#[derive(Clone, Debug)]
/// A request for key group membership.
pub struct MembershipRequestView(Document);

#[allow(dead_code)]
impl MembershipRequestView {
    /// Create a membership request view from its author and the request's document.
    pub fn new(membership_request: &Document) -> Result<Self, SystemSchemaError> {
        let view = Self(membership_request.clone());
        view.validate()?;
        Ok(view)
    }

    /// The id of this membership request view.
    pub fn view_id(&self) -> &DocumentViewId {
        self.0.view().id()
    }

    /// The key group of this membership request view.
    pub fn key_group(&self) -> &DocumentId {
        match self.0.view().get("key_group") {
            Some(OperationValue::Relation(relation)) => relation.document_id(),
            // This code is unreachable as a `MembershipRequestView` can only be created via
            // its constructor and the `TryFrom<Document>` impl, both of which check that this
            // field exists
            _ => panic!(),
        }
    }

    /// The public key or key group that is requesting membership.
    pub fn member(&self) -> Owner {
        match self.0.view().get("member") {
            Some(OperationValue::Owner(relation)) => {
                Owner::KeyGroup(relation.document_id().clone())
            }
            // This code is unreachable as a `MembershipRequestView` can only be created via
            // its constructor and the `TryFrom<Document>` impl, both of which check that this
            // field either contains `Some(OperationValue::Owner)` or `None`.
            Some(_) => panic!(),
            None => Owner::Author(self.0.author().clone()),
        }
    }
}

impl Validate for MembershipRequestView {
    type Error = SystemSchemaError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.0.is_deleted() {
            return Err(SystemSchemaError::Deleted(self.0.id().clone()));
        }

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
            // optional field, so `None` is ok as well
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
    use crate::schema::SchemaId;
    use crate::test_utils::constants::DEFAULT_HASH;
    use crate::test_utils::fixtures::{document, fields, key_pair};
    use crate::test_utils::utils::create_operation;

    use super::*;

    #[rstest]
    // Correct fields defined - this should pass
    #[case(vec![("key_group", OperationValue::Relation(DEFAULT_HASH.parse::<DocumentId>().unwrap().into()))], None)]
    // No `key_group` field is defined
    #[case(vec![("badoozle", OperationValue::Boolean(true))], Some("missing field 'key_group'"))]
    // `key_group` is defined, but it has the wrong `OperationValue` assigned to it
    #[case(vec![("key_group", OperationValue::Boolean(true))], Some("invalid field 'key_group' with value Boolean(true)"))]
    fn from_document(
        #[case] doc_fields: Vec<(&str, OperationValue)>,
        key_pair: KeyPair,
        #[case] expected_err: Option<&str>,
    ) {
        let doc = document(
            create_operation(SchemaId::KeyGroupRequest, fields(doc_fields)),
            key_pair,
            false,
        );
        let result = MembershipRequestView::try_from(doc);
        match expected_err {
            Some(err_str) => {
                assert_eq!(format!("{}", result.unwrap_err()), err_str)
            }
            None => assert!(result.is_ok()),
        };
    }

    #[rstest]
    fn deleted_doc(key_pair: KeyPair) {
        let request_doc = document(
            create_operation(
                SchemaId::KeyGroupRequest,
                fields(vec![(
                    "key_group",
                    OperationValue::Relation(DEFAULT_HASH.parse::<DocumentId>().unwrap().into()),
                )]),
            ),
            key_pair,
            true,
        );
        let result = MembershipRequestView::try_from(request_doc);
        assert_eq!(
            format!("{}", result.unwrap_err()),
            "unable to create view for deleted document DocumentId(Oper\
                ationId(Hash(\"0020630ba350c57b793aec0324e62b32ab1d8b30\
                42a9c9d215247ed7e3916ff257d9\")))"
        );
    }
}
