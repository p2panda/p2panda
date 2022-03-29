// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use crate::document::{Document, DocumentId, DocumentViewId};
use crate::operation::OperationValue;
use crate::permissions::key_group::error::KeyGroupError;
use crate::schema::system::SystemSchemaError;
use crate::Validate;

/// Represents a root key group definition.
///
/// Can be used to make a [`KeyGroup`][`crate::permissions::key_group::KeyGroup`].
///
/// Create this from a `key_group_v1` document:
///
/// ```
/// # use std::convert::TryFrom;
/// # use p2panda_rs::operation::OperationValue;
/// # use p2panda_rs::permissions::key_group::KeyGroupView;
/// # use p2panda_rs::schema::SchemaId;
/// # use p2panda_rs::test_utils::utils::{create_operation, document, operation_fields};
/// # let key_group_doc = document(
/// #     create_operation(SchemaId::KeyGroup, operation_fields(vec![
/// #         ("name", OperationValue::Text("My key group!".to_string()))
/// #     ])),
/// # );
/// let key_group_view = KeyGroupView::try_from(key_group_doc).unwrap();
/// assert_eq!(key_group_view.name(), "My key group!");
/// ```
#[derive(Clone, Debug)]
pub struct KeyGroupView(Document);

#[allow(dead_code)]
impl KeyGroupView {
    /// The id the key group.
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
            // This code is unreachable as a key group view can only be created via
            // `TryFrom<Document>`, which validates that this field exists.
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
            return Err(SystemSchemaError::Deleted(self.0.id().clone()));
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

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use rstest::rstest;

    use crate::identity::KeyPair;
    use crate::operation::OperationValue;
    use crate::permissions::key_group::{KeyGroup, KeyGroupView};
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::{create_operation, document, fields, key_group, key_pair};

    #[rstest]
    fn basics(key_group: KeyGroup) {
        assert_eq!(key_group.name(), "The Ants");
        key_group.view_id();
    }

    #[rstest]
    #[case(None, "missing field 'name'")]
    #[case(
        Some(OperationValue::Boolean(true)),
        "invalid field 'name' with value Boolean(true)"
    )]
    #[case(Some(OperationValue::Text("".to_string())), "invalid field 'name' with value Text(\"\")")]
    fn view_basics(
        #[case] name: Option<OperationValue>,
        key_pair: KeyPair,
        #[case] expected_err: String,
    ) {
        let doc_fields = match name {
            Some(value) => vec![("name", value)],
            None => vec![("badoozle", OperationValue::Integer(0))],
        };
        let key_group_doc = document(
            create_operation(SchemaId::KeyGroup, fields(doc_fields)),
            key_pair,
            false,
        );
        let result = KeyGroupView::try_from(key_group_doc);
        assert_eq!(format!("{}", result.unwrap_err()), expected_err);
    }

    #[rstest]
    fn deleted_doc(key_pair: KeyPair) {
        let key_group_doc = document(
            create_operation(
                SchemaId::KeyGroup,
                fields(vec![("name", OperationValue::Text("Test".to_string()))]),
            ),
            key_pair,
            true,
        );
        let result = KeyGroupView::try_from(key_group_doc);
        assert_eq!(
            format!("{}", result.unwrap_err()),
            "unable to create view for deleted document \
        DocumentId(OperationId(Hash(\"0020655926244370ace06086e934b54bd69a6e9ab38458356c6217a13238\
        120d9621\")))"
        );
    }
}
