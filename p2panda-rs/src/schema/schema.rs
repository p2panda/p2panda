// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

<<<<<<< HEAD
use crate::cddl::generate_cddl_definition;
use crate::document::DocumentViewId;
use crate::schema::system::{FieldType, SchemaFieldView, SchemaView};
=======
use crate::document::DocumentViewId;
use crate::schema::system::{SchemaFieldView, SchemaView};
>>>>>>> Introduce `Schema` struct (again...)
use crate::schema::SchemaError;

/// The key of a schema field
type FieldKey = String;

/// A struct representing a materialised schema.
///
/// It is constructed from a `SchemaView` and all related `SchemaFieldView`s.
#[derive(Debug, PartialEq)]
pub struct Schema {
    id: DocumentViewId,
    name: String,
    description: String,
<<<<<<< HEAD
    fields: BTreeMap<FieldKey, FieldType>,
=======
    fields: BTreeMap<FieldKey, SchemaFieldView>,
>>>>>>> Introduce `Schema` struct (again...)
}

impl Schema {
    /// Instantiate a new `Schema` from a `SchemaView` and it's `SchemaFieldView`s
<<<<<<< HEAD
    #[allow(unused)]
=======
>>>>>>> Introduce `Schema` struct (again...)
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
<<<<<<< HEAD
            fields_map.insert(field.name().to_string(), field.field_type().to_owned());
=======
            fields_map.insert(field.name().to_string(), field);
>>>>>>> Introduce `Schema` struct (again...)
        }

        Ok(Schema {
            id: schema.view_id().to_owned(),
            name: schema.name().to_owned(),
            description: schema.description().to_owned(),
            fields: fields_map,
        })
    }
<<<<<<< HEAD

    /// Return a definition for this schema expressed as a CDDL string.
    #[allow(unused)]
    pub fn as_cddl(&self) -> String {
        generate_cddl_definition(&self.fields)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::convert::TryInto;

    use rstest::rstest;

    use crate::document::{DocumentId, DocumentView, DocumentViewId};
    use crate::hash::Hash;
    use crate::operation::{OperationValue, OperationValueRelationList, PinnedRelationList};
    use crate::schema::schema::Schema;
    use crate::schema::system::{SchemaFieldView, SchemaView};
    use crate::test_utils::fixtures::{random_document_id, random_hash};

    fn create_schema(
        fields: PinnedRelationList,
        view_id: DocumentViewId,
        document_id: DocumentId,
    ) -> SchemaView {
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
            OperationValue::RelationList(OperationValueRelationList::Pinned(fields)),
        );
        let schema_view: SchemaView = DocumentView::new(view_id, document_id, schema)
            .try_into()
            .unwrap();
        schema_view
    }

    fn create_field(
        name: &str,
        field_type: &str,
        view_id: DocumentViewId,
        document_id: DocumentId,
    ) -> SchemaFieldView {
        let mut capacity_field = BTreeMap::new();
        capacity_field.insert("name".to_string(), OperationValue::Text(name.to_string()));
        capacity_field.insert(
            "type".to_string(),
            OperationValue::Text(field_type.to_string()),
        );

        let capacity_field_view: SchemaFieldView =
            DocumentView::new(view_id, document_id, capacity_field)
                .try_into()
                .unwrap();
        capacity_field_view
    }

    #[rstest]
    fn construct_schema(
        #[from(random_hash)] relation_operation_id_1: Hash,
        #[from(random_hash)] relation_operation_id_2: Hash,
        #[from(random_hash)] relation_operation_id_3: Hash,
        #[from(random_hash)] schema_view_id: Hash,
        #[from(random_document_id)] schema_id: DocumentId,
        #[from(random_document_id)] bool_field_id: DocumentId,
        #[from(random_document_id)] capacity_field_id: DocumentId,
    ) {
        // Create schema definition for "venue"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let schema_view_id = DocumentViewId::new(vec![schema_view_id]);
        let fields = PinnedRelationList::new(vec![
            DocumentViewId::new(vec![relation_operation_id_1.clone()]),
            DocumentViewId::new(vec![
                relation_operation_id_2.clone(),
                relation_operation_id_3.clone(),
            ]),
        ]);

        let schema_view = create_schema(fields, schema_view_id, schema_id);

        // Create first schema field "is_accessible"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let bool_field_view = create_field(
            "is_accessible",
            "bool",
            DocumentViewId::new(vec![relation_operation_id_1]),
            bool_field_id,
        );

        // Create second schema field "capacity"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let capacity_field_view = create_field(
            "capacity",
            "int",
            DocumentViewId::new(vec![relation_operation_id_2, relation_operation_id_3]),
            capacity_field_id,
        );

        // Create venue schema from schema and field views
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let schema = Schema::new(schema_view, vec![bool_field_view, capacity_field_view]);

        // Schema should be ok
        assert!(schema.is_ok());

        let expected_cddl = "capacity = { type: \"int\", value: int, }\n".to_string()
            + "is_accessible = { type: \"bool\", value: bool, }\n"
            + "create-fields = { capacity, is_accessible }\n"
            + "update-fields = { + ( capacity // is_accessible ) }";

        // Schema should return correct cddl string
        assert_eq!(expected_cddl, schema.unwrap().as_cddl());
    }

    #[rstest]
    fn invalid_fields_fail(
        #[from(random_hash)] relation_operation_id_1: Hash,
        #[from(random_hash)] relation_operation_id_2: Hash,
        #[from(random_hash)] invalid_relation_hash: Hash,
        #[from(random_hash)] schema_view_id: Hash,
        #[from(random_document_id)] schema_id: DocumentId,
        #[from(random_document_id)] bool_field_id: DocumentId,
        #[from(random_document_id)] capacity_field_id: DocumentId,
    ) {
        // Create schema definition for "venue"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let schema_view_id = DocumentViewId::new(vec![schema_view_id]);
        let fields = PinnedRelationList::new(vec![
            DocumentViewId::new(vec![relation_operation_id_1.clone()]),
            DocumentViewId::new(vec![relation_operation_id_2.clone()]),
        ]);

        let schema_view = create_schema(fields, schema_view_id, schema_id);

        // Create first valid schema field "is_accessible"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let bool_field_document_view_id = DocumentViewId::new(vec![relation_operation_id_1]);
        let bool_field_view = create_field(
            "is_accessible",
            "bool",
            bool_field_document_view_id,
            bool_field_id,
        );

        // Create second valid schema field "capacity"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let capacity_field_document_view_id = DocumentViewId::new(vec![relation_operation_id_2]);
        let capacity_field_view = create_field(
            "capacity",
            "int",
            capacity_field_document_view_id,
            capacity_field_id.clone(),
        );

        // Create field with invalid DocumentViewId
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let invalid_document_view_id = DocumentViewId::new(vec![invalid_relation_hash]);
        let field_with_invalid_document_view_id = create_field(
            "capacity",
            "int",
            invalid_document_view_id,
            capacity_field_id,
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
=======
>>>>>>> Introduce `Schema` struct (again...)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::convert::TryInto;

    use rstest::rstest;

    use crate::document::{DocumentId, DocumentView, DocumentViewId};
    use crate::hash::Hash;
    use crate::operation::{OperationValue, OperationValueRelationList, PinnedRelationList};
    use crate::schema::schema::Schema;
    use crate::schema::system::{SchemaFieldView, SchemaView};
    use crate::test_utils::fixtures::{random_document_id, random_hash};

    #[rstest]
    fn construct_schema(
        #[from(random_hash)] relation_operation_id_1: Hash,
        #[from(random_hash)] relation_operation_id_2: Hash,
        #[from(random_hash)] relation_operation_id_3: Hash,
        #[from(random_hash)] relation_operation_id_4: Hash,
        #[from(random_hash)] schema_view_id: Hash,
        #[from(random_document_id)] schema_id: DocumentId,
        #[from(random_document_id)] bool_field_id: DocumentId,
        #[from(random_document_id)] capacity_field_id: DocumentId,
        #[from(random_document_id)] toilet_size_field_id: DocumentId,
    ) {
        // Create schema definition for "venue"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

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
            OperationValue::RelationList(OperationValueRelationList::Pinned(
                PinnedRelationList::new(vec![
                    DocumentViewId::new(vec![relation_operation_id_1.clone()]),
                    DocumentViewId::new(vec![
                        relation_operation_id_2.clone(),
                        relation_operation_id_3.clone(),
                    ]),
                ]),
            )),
        );

        let schema_view_id = DocumentViewId::new(vec![schema_view_id]);
        let schema_view: SchemaView = DocumentView::new(schema_view_id, schema_id, schema)
            .try_into()
            .unwrap();

        // Create first schema field "is_accessible"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let mut bool_field = BTreeMap::new();
        bool_field.insert(
            "name".to_string(),
            OperationValue::Text("is_accessible".to_string()),
        );
        bool_field.insert("type".to_string(), OperationValue::Text("bool".to_string()));

        let bool_field_view_id = DocumentViewId::new(vec![relation_operation_id_1]);
        let bool_field_view: SchemaFieldView =
            DocumentView::new(bool_field_view_id, bool_field_id, bool_field)
                .try_into()
                .unwrap();

        // Create second schema field "capacity"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let mut capacity_field = BTreeMap::new();
        capacity_field.insert(
            "name".to_string(),
            OperationValue::Text("capacity".to_string()),
        );
        capacity_field.insert("type".to_string(), OperationValue::Text("int".to_string()));

        let capacity_field_view_id =
            DocumentViewId::new(vec![relation_operation_id_2, relation_operation_id_3]);
        let capacity_field_view: SchemaFieldView =
            DocumentView::new(capacity_field_view_id, capacity_field_id, capacity_field)
                .try_into()
                .unwrap();

        // Create venue schema from schema and field views
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        assert!(Schema::new(
            schema_view.clone(),
            vec![bool_field_view.clone(), capacity_field_view.clone()]
        )
        .is_ok());

        // Invalid fields should fail
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let mut toilet_size_field = BTreeMap::new();
        toilet_size_field.insert(
            "name".to_string(),
            OperationValue::Text("toilet_size".to_string()),
        );
        toilet_size_field.insert("type".to_string(), OperationValue::Text("int".to_string()));

        let toilet_size_field_view_id = DocumentViewId::new(vec![relation_operation_id_4]);
        let toilet_size_field_view: SchemaFieldView = DocumentView::new(
            toilet_size_field_view_id,
            toilet_size_field_id,
            toilet_size_field,
        )
        .try_into()
        .unwrap();

        // Invalid field passed
        assert!(Schema::new(
            schema_view.clone(),
            vec![bool_field_view.clone(), toilet_size_field_view.clone()]
        )
        .is_err());
        // Too few fields passed
        assert!(Schema::new(schema_view.clone(), vec![bool_field_view.clone()]).is_err());
        // Too many fields passed
        assert!(Schema::new(
            schema_view,
            vec![bool_field_view, capacity_field_view, toilet_size_field_view]
        )
        .is_err());
    }
}
