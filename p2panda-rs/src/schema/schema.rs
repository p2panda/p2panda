// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::fmt::Display;

use crate::cddl::generate_cddl_definition;
use crate::document::DocumentViewId;
use crate::schema::system::{SchemaFieldView, SchemaView};
use crate::schema::{FieldType, SchemaError};

/// The key of a schema field
type FieldKey = String;

/// A struct representing a materialised schema.
///
/// It is constructed from a [`SchemaView`] and all related [`SchemaFieldView`]s.
#[derive(Clone, Debug, PartialEq)]
pub struct Schema {
    id: DocumentViewId,
    name: String,
    description: String,
    fields: BTreeMap<FieldKey, FieldType>,
}

impl Schema {
    /// Instantiate a new `Schema` from a `SchemaView` and it's `SchemaFieldView`s
    #[allow(unused)]
    pub fn new(schema: SchemaView, fields: Vec<SchemaFieldView>) -> Result<Schema, SchemaError> {
        // Validate that the passed `SchemaFields` are the correct ones for this `Schema`.
        for schema_field in schema.fields().iter() {
            match fields
                .iter()
                .find(|schema_field_view| schema_field_view.id() == &schema_field)
            {
                Some(_) => Ok(()),
                None => Err(SchemaError::InvalidFields),
            }?;
        }

        // And that no extra fields were passed
        if fields.iter().len() > schema.fields().iter().len() {
            return Err(SchemaError::InvalidFields);
        }

        // Construct a key-value map of fields
        let mut fields_map = BTreeMap::new();
        for field in fields {
            fields_map.insert(field.name().to_string(), field.field_type().to_owned());
        }

        Ok(Schema {
            id: schema.view_id().to_owned(),
            name: schema.name().to_owned(),
            description: schema.description().to_owned(),
            fields: fields_map,
        })
    }

    /// Return a definition for this schema expressed as a CDDL string.
    #[allow(unused)]
    pub fn as_cddl(&self) -> String {
        generate_cddl_definition(&self.fields)
    }

    /// Returns a unique string identifier for this schema.
    ///
    /// This identifier can only be used when it is not necessary to reconstruct this schema's
    /// document from it.
    ///
    /// It has the format "<schema name>__<hashed schema document view graph tips>".
    #[allow(unused)]
    pub fn hash_id(&self) -> String {
        format!("{}__{}", self.name, self.id.hash().as_str())
    }
}

impl Display for Schema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}_{}", self.name, self.id)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::convert::TryInto;

    use rstest::rstest;

    use crate::document::{DocumentView, DocumentViewId};
    use crate::operation::{OperationId, OperationValue, PinnedRelationList};
    use crate::schema::schema::Schema;
    use crate::schema::system::{SchemaFieldView, SchemaView};
    use crate::test_utils::fixtures::{document_view_id, random_operation_id};

    fn create_schema(fields: PinnedRelationList, view_id: DocumentViewId) -> SchemaView {
        let mut schema = BTreeMap::new();
        schema.insert(
            "name".to_string(),
            OperationValue::Text("venue_name".to_string()),
        );
        schema.insert(
            "description".to_string(),
            OperationValue::Text("Describes a venue".to_string()),
        );
        schema.insert(
            "fields".to_string(),
            OperationValue::PinnedRelationList(fields),
        );
        let schema_view: SchemaView = DocumentView::new(view_id, schema).try_into().unwrap();
        schema_view
    }

    fn create_field(name: &str, field_type: &str, view_id: DocumentViewId) -> SchemaFieldView {
        let mut capacity_field = BTreeMap::new();
        capacity_field.insert("name".to_string(), OperationValue::Text(name.to_string()));
        capacity_field.insert(
            "type".to_string(),
            OperationValue::Text(field_type.to_string()),
        );

        let capacity_field_view: SchemaFieldView = DocumentView::new(view_id, capacity_field)
            .try_into()
            .unwrap();
        capacity_field_view
    }

    #[rstest]
    fn construct_schema(
        #[from(random_operation_id)] relation_operation_id_1: OperationId,
        #[from(random_operation_id)] relation_operation_id_2: OperationId,
        #[from(random_operation_id)] relation_operation_id_3: OperationId,
        #[from(document_view_id)] schema_view_id: DocumentViewId,
    ) {
        // Create schema definition for "venue"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let fields = PinnedRelationList::new(vec![
            DocumentViewId::new(&[relation_operation_id_1.clone()]),
            DocumentViewId::new(&[
                relation_operation_id_2.clone(),
                relation_operation_id_3.clone(),
            ]),
        ]);

        let schema_view = create_schema(fields, schema_view_id);

        // Create first schema field "is_accessible"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let bool_field_view = create_field(
            "is_accessible",
            "bool",
            DocumentViewId::from(relation_operation_id_1),
        );

        // Create second schema field "capacity"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let capacity_field_view = create_field(
            "capacity",
            "int",
            DocumentViewId::new(&[relation_operation_id_2, relation_operation_id_3]),
        );

        // Create venue schema from schema and field views
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let result = Schema::new(schema_view, vec![bool_field_view, capacity_field_view]);

        // Schema should be ok
        assert!(result.is_ok());

        let schema = result.unwrap();

        let expected_cddl = "capacity = { type: \"int\", value: int, }\n".to_string()
            + "is_accessible = { type: \"bool\", value: bool, }\n"
            + "create-fields = { capacity, is_accessible }\n"
            + "update-fields = { + ( capacity // is_accessible ) }";

        // Schema should return correct cddl string
        assert_eq!(expected_cddl, schema.as_cddl());

        // Schema should have a string representation
        assert!(format!("{}", schema).starts_with("venue_name_0020"));

        // Schema should have a hash id
        assert!(schema.hash_id().starts_with("venue_name__0020"));
    }

    #[rstest]
    fn invalid_fields_fail(
        #[from(random_operation_id)] relation_operation_id_1: OperationId,
        #[from(random_operation_id)] relation_operation_id_2: OperationId,
        #[from(random_operation_id)] invalid_relation_id: OperationId,
        #[from(document_view_id)] schema_view_id: DocumentViewId,
    ) {
        // Create schema definition for "venue"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let fields = PinnedRelationList::new(vec![
            DocumentViewId::from(relation_operation_id_1.clone()),
            DocumentViewId::from(relation_operation_id_2.clone()),
        ]);

        let schema_view = create_schema(fields, schema_view_id);

        // Create first valid schema field "is_accessible"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let bool_field_document_view_id = DocumentViewId::from(relation_operation_id_1);
        let bool_field_view = create_field("is_accessible", "bool", bool_field_document_view_id);

        // Create second valid schema field "capacity"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let capacity_field_document_view_id = DocumentViewId::from(relation_operation_id_2);
        let capacity_field_view = create_field("capacity", "int", capacity_field_document_view_id);

        // Create field with invalid DocumentViewId
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let invalid_document_view_id = DocumentViewId::from(invalid_relation_id);
        let field_with_invalid_document_view_id =
            create_field("capacity", "int", invalid_document_view_id);

        // Passing field with invalid DocumentViewId should fail
        assert!(Schema::new(
            schema_view.clone(),
            vec![
                bool_field_view.clone(),
                field_with_invalid_document_view_id.clone()
            ]
        )
        .is_err());

        // Passing too few fields should fail
        assert!(Schema::new(schema_view.clone(), vec![bool_field_view.clone()]).is_err());

        // Passing too many fields should fail
        assert!(Schema::new(
            schema_view,
            vec![
                bool_field_view,
                capacity_field_view,
                field_with_invalid_document_view_id
            ]
        )
        .is_err());
    }
}
