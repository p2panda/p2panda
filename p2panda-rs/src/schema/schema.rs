// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::fmt::Display;

use crate::cddl::generate_cddl_definition;
use crate::schema::system::{SchemaFieldView, SchemaView};
use crate::schema::{FieldType, SchemaError, SchemaId, SchemaVersion};

/// The key of a schema field
type FieldKey = String;

/// A struct representing a materialised schema.
///
/// It is constructed from a [`SchemaView`] and all related [`SchemaFieldView`]s.
#[derive(Clone, Debug, PartialEq)]
pub struct Schema {
    /// The application schema id for this schema.
    id: SchemaId,

    /// Describes the schema's intended use.
    description: String,

    /// Maps all of the schema's field names to their respective types.
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
            id: SchemaId::new_application(schema.name(), schema.view_id()),
            description: schema.description().to_owned(),
            fields: fields_map,
        })
    }

    /// Return a definition for this schema expressed as a CDDL string.
    #[allow(unused)]
    pub fn as_cddl(&self) -> String {
        generate_cddl_definition(&self.fields)
    }

    /// Access the schema's [`SchemaId`].
    #[allow(unused)]
    pub fn id(&self) -> &SchemaId {
        &self.id
    }

    /// Access the schema version.
    #[allow(unused)]
    pub fn version(&self) -> SchemaVersion {
        self.id.version()
    }

    /// Access the schema name.
    #[allow(unused)]
    pub fn name(&self) -> &str {
        self.id.name()
    }

    /// Access the schema description.
    #[allow(unused)]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Access the schema fields.
    #[allow(unused)]
    pub fn fields(&self) -> &BTreeMap<FieldKey, FieldType> {
        &self.fields
    }
}

impl Display for Schema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<Schema {}>", self.id)
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use rstest::rstest;

    use crate::document::{DocumentView, DocumentViewFields, DocumentViewId, DocumentViewValue};
    use crate::operation::{OperationId, OperationValue, PinnedRelationList};
    use crate::schema::system::{SchemaFieldView, SchemaView};
    use crate::schema::{Schema, SchemaId, SchemaVersion};
    use crate::test_utils::fixtures::{document_view_id, random_operation_id};

    fn create_schema_view(
        fields: PinnedRelationList,
        view_id: DocumentViewId,
        operation_id: OperationId,
    ) -> SchemaView {
        let mut schema = DocumentViewFields::new();
        schema.insert(
            "name",
            DocumentViewValue::Value(
                operation_id.clone(),
                OperationValue::Text("venue_name".to_string()),
            ),
        );
        schema.insert(
            "description",
            DocumentViewValue::Value(
                operation_id.clone(),
                OperationValue::Text("Describes a venue".to_string()),
            ),
        );
        schema.insert(
            "fields",
            DocumentViewValue::Value(operation_id, OperationValue::PinnedRelationList(fields)),
        );
        let schema_view: SchemaView = DocumentView::new(view_id, schema).try_into().unwrap();
        schema_view
    }

    fn create_field(
        name: &str,
        field_type: &str,
        view_id: DocumentViewId,
        operation_id: OperationId,
    ) -> SchemaFieldView {
        let mut capacity_field = DocumentViewFields::new();
        capacity_field.insert(
            "name",
            DocumentViewValue::Value(operation_id.clone(), OperationValue::Text(name.to_string())),
        );
        capacity_field.insert(
            "type",
            DocumentViewValue::Value(operation_id, OperationValue::Text(field_type.to_string())),
        );

        let capacity_field_view: SchemaFieldView = DocumentView::new(view_id, capacity_field)
            .try_into()
            .unwrap();
        capacity_field_view
    }

    #[rstest]
    fn construct_schema(
        #[from(random_operation_id)] field_operation_id: OperationId,
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

        let schema_view = create_schema_view(fields, schema_view_id, field_operation_id.clone());

        // Create first schema field "is_accessible"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let bool_field_view = create_field(
            "is_accessible",
            "bool",
            DocumentViewId::from(relation_operation_id_1),
            field_operation_id.clone(),
        );

        // Create second schema field "capacity"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let capacity_field_view = create_field(
            "capacity",
            "int",
            DocumentViewId::new(&[relation_operation_id_2, relation_operation_id_3]),
            field_operation_id,
        );

        // Create venue schema from schema and field views
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let result = Schema::new(schema_view, vec![bool_field_view, capacity_field_view]);

        // Schema should be ok
        assert!(result.is_ok());

        let schema = result.unwrap();

        // Test getters
        let expected_view_id =
            "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543"
                .parse::<DocumentViewId>()
                .unwrap();
        assert_eq!(
            schema.id(),
            &SchemaId::new_application("venue_name", &expected_view_id)
        );
        assert_eq!(schema.name(), "venue_name");
        assert_eq!(
            schema.version(),
            SchemaVersion::Application(expected_view_id)
        );
        assert_eq!(schema.description(), "Describes a venue");
        assert_eq!(schema.fields().len(), 2);

        let expected_cddl = "capacity = { type: \"int\", value: int, }\n".to_string()
            + "is_accessible = { type: \"bool\", value: bool, }\n"
            + "create-fields = { capacity, is_accessible }\n"
            + "update-fields = { + ( capacity // is_accessible ) }";

        // Schema should return correct cddl string
        assert_eq!(expected_cddl, schema.as_cddl());

        // Schema should have a string representation
        assert_eq!(format!("{}", schema), "<Schema venue_name 496543>");
    }

    #[rstest]
    fn invalid_fields_fail(
        #[from(random_operation_id)] field_operation_id: OperationId,
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

        let schema_view = create_schema_view(fields, schema_view_id, field_operation_id.clone());

        // Create first valid schema field "is_accessible"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let bool_field_document_view_id = DocumentViewId::from(relation_operation_id_1);
        let bool_field_view = create_field(
            "is_accessible",
            "bool",
            bool_field_document_view_id,
            field_operation_id.clone(),
        );

        // Create second valid schema field "capacity"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let capacity_field_document_view_id = DocumentViewId::from(relation_operation_id_2);
        let capacity_field_view = create_field(
            "capacity",
            "int",
            capacity_field_document_view_id,
            field_operation_id.clone(),
        );

        // Create field with invalid DocumentViewId
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let invalid_document_view_id = DocumentViewId::from(invalid_relation_id);
        let field_with_invalid_document_view_id = create_field(
            "capacity",
            "int",
            invalid_document_view_id,
            field_operation_id.clone(),
        );

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
