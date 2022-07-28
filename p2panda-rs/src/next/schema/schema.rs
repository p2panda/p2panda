// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::fmt::Display;

use crate::next::document::DocumentViewHash;
use crate::next::schema::error::{SchemaError, SchemaIdError};
use crate::next::schema::system::{
    get_schema_definition, get_schema_field_definition, SchemaFieldView, SchemaView,
};
use crate::next::schema::{FieldType, SchemaId, SchemaVersion};
use crate::Human;

/// The key of a schema field
pub type FieldName = String;

/// A struct representing a p2panda schema.
///
/// ## Load application schemas from document views
///
/// In most cases you should construct schema instances from their materialised views to ensure
/// that your definition aligns with a published version of a schema.
///
/// Use [`Schema::from_views`] to infer a schema instance from a [`SchemaView`] and all related
/// [`SchemaFieldView`]s.
///
/// ## Access system schemas
///
/// Use [`Schema::get_system`] to access static definitions of all system schemas available in this
/// version of the p2panda library.
///
/// ## Define a schema without going through document views
///
/// [`Schema::new`] is only available for testing. This method of constructing a schema doesn't
/// validate that the given schema id matches the provided schema's published description and field
/// definitions.
///
// @NOTE: Fields on this struct are `pub(super)` to enable making static instances of system
// schemas from their respective files in the `./system` subdirectory. Making system schema
// instances is not supported by `Schema::new()` to prevent their dynamic redefinition.
#[derive(Clone, Debug, PartialEq)]
pub struct Schema {
    /// The application schema id for this schema.
    pub(super) id: SchemaId,

    /// Describes the schema's intended use.
    pub(super) description: String,

    /// Maps all of the schema's field names to their respective types.
    pub(super) fields: BTreeMap<FieldName, FieldType>,
}

impl Schema {
    /// Create an application schema instance with the given id, description and fields.
    ///
    /// Use [`Schema::get_system`] to access static system schema instances.
    ///
    /// ## Example
    ///
    /// ```
    /// # #[cfg(test)]
    /// # mod doc_test {
    /// # extern crate p2panda_rs;
    /// # use p2panda_rs::document::DocumentViewId;
    /// # use p2panda_rs::test_utils::fixtures::{document_view_id};
    /// #
    /// # #[rstest]
    /// # fn main(#[from(document_view_id)] schema_document_view_id: DocumentViewId) {
    /// let schema = Schema::new(
    ///     SchemaId::Application("cucumber", schema_document_view_id),
    ///     "A variety in the cucumber society's database.",
    ///     vec![
    ///         ("name", FieldType::String),
    ///         ("grow_cycle_days", FieldType::Int),
    ///         ("flavor_rating", FieldType::Int),
    ///     ]
    /// );
    /// assert!(schema.is_ok());
    /// # }
    /// # }
    /// ```
    #[cfg(any(feature = "testing", test))]
    pub fn new(
        id: &SchemaId,
        description: &str,
        fields: Vec<(impl ToString, FieldType)>,
    ) -> Result<Self, SchemaError> {
        let mut field_map: BTreeMap<String, FieldType> = BTreeMap::new();

        for (field_name, field_type) in fields {
            field_map.insert(field_name.to_string(), field_type.to_owned());
        }

        if let SchemaId::Application(_, _) = id {
            let schema = Self {
                id: id.to_owned(),
                description: description.to_owned(),
                fields: field_map,
            };

            // @TODO: Implement `Validate` for `Schema` and call it here

            Ok(schema)
        } else {
            Err(SchemaError::DynamicSystemSchema(id.clone()))
        }
    }

    /// Instantiate a new `Schema` from a `SchemaView` and it's `SchemaFieldView`s.
    pub fn from_views(
        schema: SchemaView,
        fields: Vec<SchemaFieldView>,
    ) -> Result<Schema, SchemaError> {
        let schema_fields_iter = schema.fields().to_owned().into_iter();
        let schema_fields_len = schema_fields_iter.len();

        // Validate that the passed `SchemaFields` are the correct ones for this `Schema`.
        for schema_field in schema_fields_iter {
            match fields
                .iter()
                .find(|schema_field_view| schema_field_view.id() == &schema_field)
            {
                Some(_) => Ok(()),
                None => Err(SchemaError::InvalidFields),
            }?;
        }

        // And that no extra fields were passed
        if fields.iter().len() > schema_fields_len {
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

    /// Return a static `Schema` instance for a system schema.
    ///
    /// Returns an error if this library version doesn't support the system schema with the given
    /// version.
    ///
    /// ## Example
    ///
    /// Get a `Schema` instance for version 1 of the _schema definition_ schema:
    ///
    /// ```
    /// # extern crate p2panda_rs;
    /// # use p2panda_rs::schema::{Schema, SchemaId};
    /// let schema_definition = Schema::get_system(SchemaId::SchemaDefinition(1));
    /// assert!(schema_definition.is_ok());
    /// ```
    pub fn get_system(schema_id: SchemaId) -> Result<&'static Schema, SchemaIdError> {
        match schema_id {
            SchemaId::SchemaDefinition(version) => get_schema_definition(version),
            SchemaId::SchemaFieldDefinition(version) => get_schema_field_definition(version),
            _ => Err(SchemaIdError::UnknownSystemSchema(schema_id.to_string())),
        }
    }

    /// Access the schema's [`SchemaId`].
    pub fn id(&self) -> &SchemaId {
        &self.id
    }

    /// Returns a unique string identifier for this schema.
    ///
    /// This identifier can only be used when it is not necessary to reconstruct this schema's
    /// document from it.
    ///
    /// It has the format "<schema name>__<hashed schema document view>" for application schemas
    /// and "<schema_name>__<version>" for system schemas (note that this has two underscores,
    /// while schema id has only one).
    pub fn hash_id(&self) -> String {
        match self.id.version() {
            SchemaVersion::Application(view_id) => {
                format!("{}__{}", self.name(), DocumentViewHash::from(&view_id))
            }
            SchemaVersion::System(version) => {
                format!("{}__{}", self.name(), version.to_string())
            }
        }
    }

    /// Access the schema version.
    pub fn version(&self) -> SchemaVersion {
        self.id.version()
    }

    /// Access the schema name.
    pub fn name(&self) -> &str {
        self.id.name()
    }

    /// Access the schema description.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Access the schema fields.
    pub fn fields(&self) -> &BTreeMap<FieldName, FieldType> {
        &self.fields
    }
}

impl Display for Schema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl Human for Schema {
    fn display(&self) -> String {
        format!("<Schema {}>", self.id.display())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::convert::TryInto;

    use rstest::rstest;

    use crate::next::document::DocumentViewId;
    use crate::next::document::{DocumentView, DocumentViewFields, DocumentViewValue};
    use crate::next::operation::{OperationId, OperationValue, PinnedRelationList};
    use crate::next::schema::system::{SchemaFieldView, SchemaView};
    use crate::next::schema::{FieldType, Schema, SchemaId, SchemaVersion};
    use crate::next::test_utils::fixtures::{document_view_id, random_operation_id};
    use crate::Human;

    fn create_schema_view(
        fields: &PinnedRelationList,
        view_id: &DocumentViewId,
        operation_id: &OperationId,
    ) -> SchemaView {
        let mut schema = DocumentViewFields::new();

        schema.insert(
            "name",
            DocumentViewValue::new(
                operation_id,
                &OperationValue::String("venue_name".to_string()),
            ),
        );
        schema.insert(
            "description",
            DocumentViewValue::new(
                operation_id,
                &OperationValue::String("Describes a venue".to_string()),
            ),
        );
        schema.insert(
            "fields",
            DocumentViewValue::new(
                operation_id,
                &OperationValue::PinnedRelationList(fields.clone()),
            ),
        );

        let schema_view: SchemaView = DocumentView::new(view_id, &schema).try_into().unwrap();
        schema_view
    }

    fn create_field(
        name: &str,
        field_type: &str,
        view_id: &DocumentViewId,
        operation_id: &OperationId,
    ) -> SchemaFieldView {
        let mut capacity_field = DocumentViewFields::new();
        capacity_field.insert(
            "name",
            DocumentViewValue::new(operation_id, &OperationValue::String(name.to_string())),
        );
        capacity_field.insert(
            "type",
            DocumentViewValue::new(
                operation_id,
                &OperationValue::String(field_type.to_string()),
            ),
        );

        let capacity_field_view: SchemaFieldView = DocumentView::new(view_id, &capacity_field)
            .try_into()
            .unwrap();
        capacity_field_view
    }

    #[rstest]
    fn string_representation(#[from(document_view_id)] schema_view_id: DocumentViewId) {
        let schema = Schema::new(
            &SchemaId::Application("venue".into(), schema_view_id),
            "Some description",
            vec![("number", FieldType::Integer)],
        )
        .unwrap();

        assert_eq!(
            format!("{}", schema),
            "venue_0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543"
        );

        // Make sure the id is matching
        assert_eq!(
            schema.id().to_string(),
            "venue_0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543"
        );
    }

    #[rstest]
    fn short_representation(#[from(document_view_id)] schema_view_id: DocumentViewId) {
        let schema = Schema::new(
            &SchemaId::Application("venue".into(), schema_view_id),
            "Some description",
            vec![("number", FieldType::Integer)],
        )
        .unwrap();
        assert_eq!(schema.display(), "<Schema venue 496543>");

        let schema_definition = Schema::get_system(SchemaId::SchemaDefinition(1)).unwrap();
        assert_eq!(schema_definition.display(), "<Schema schema_definition_v1>");

        let schema_field_definition =
            Schema::get_system(SchemaId::SchemaFieldDefinition(1)).unwrap();
        assert_eq!(
            schema_field_definition.display(),
            "<Schema schema_field_definition_v1>"
        );
    }

    #[rstest]
    #[case(vec![("message", FieldType::String)])]
    // @TODO: This should error but requires validation of schema instances.
    #[case(vec![])]
    fn new_schema(
        #[from(document_view_id)] schema_view_id: DocumentViewId,
        #[case] fields: Vec<(&str, FieldType)>,
    ) {
        let result = Schema::new(
            &SchemaId::Application("venue".to_owned(), schema_view_id),
            "description",
            fields,
        );
        assert!(result.is_ok());
    }

    #[rstest]
    fn no_redefinition_of_system_schemas() {
        let result = Schema::new(
            &SchemaId::SchemaDefinition(1),
            "description",
            vec![("wrong", FieldType::Integer)],
        );
        assert_eq!(
            format!("{}", result.unwrap_err()),
            "dynamic redefinition of system schema schema_definition_v1, use `Schema::get_system` instead"
        );
    }

    #[test]
    fn test_unsupported_system_schema() {
        let result = Schema::get_system(SchemaId::SchemaDefinition(0));
        assert_eq!(
            format!("{}", result.unwrap_err()),
            "unsupported system schema: schema_definition_v0"
        );

        let result = Schema::get_system(SchemaId::SchemaFieldDefinition(0));
        assert_eq!(
            format!("{}", result.unwrap_err()),
            "unsupported system schema: schema_field_definition_v0"
        );
    }

    #[rstest]
    fn test_error_application_schema(document_view_id: DocumentViewId) {
        let schema = Schema::get_system(SchemaId::Application(
            "events".to_string(),
            document_view_id,
        ));
        assert!(schema.is_err())
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
            DocumentViewId::new(&[relation_operation_id_1.clone()]).unwrap(),
            DocumentViewId::new(&[
                relation_operation_id_2.clone(),
                relation_operation_id_3.clone(),
            ])
            .unwrap(),
        ]);

        let schema_view = create_schema_view(&fields, &schema_view_id, &field_operation_id);

        // Create first schema field "is_accessible"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let bool_field_view = create_field(
            "is_accessible",
            "bool",
            &DocumentViewId::from(relation_operation_id_1),
            &field_operation_id,
        );

        // Create second schema field "capacity"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let capacity_field_view = create_field(
            "capacity",
            "int",
            &DocumentViewId::new(&[relation_operation_id_2, relation_operation_id_3]).unwrap(),
            &field_operation_id,
        );

        // Create venue schema from schema and field views
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let result = Schema::from_views(schema_view, vec![bool_field_view, capacity_field_view]);

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
    }

    #[rstest]
    fn hash_id(#[from(document_view_id)] application_schema_view_id: DocumentViewId) {
        // Validate application schema format
        let mut schema_fields = BTreeMap::new();
        schema_fields.insert("is_real".to_string(), FieldType::Boolean);
        let application_schema = Schema {
            id: SchemaId::Application("event".to_string(), application_schema_view_id),
            description: "test".to_string(),
            fields: schema_fields.clone(),
        };
        let application_schema_hash_id = application_schema.hash_id();
        assert_eq!(
            "event__0020fc76e3a452648023d5e169369116be1526f6d3fc2b7742ed1af2b55f11bca7fb",
            application_schema_hash_id
        );

        // Validate system schema format
        let system_schema = Schema {
            id: SchemaId::SchemaDefinition(1),
            description: "test".to_string(),
            fields: schema_fields,
        };
        let system_schema_hash_id = system_schema.hash_id();
        assert_eq!("schema_definition__1", system_schema_hash_id);
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

        let schema_view = create_schema_view(&fields, &schema_view_id, &field_operation_id);

        // Create first valid schema field "is_accessible"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let bool_field_document_view_id = DocumentViewId::from(relation_operation_id_1);
        let bool_field_view = create_field(
            "is_accessible",
            "bool",
            &bool_field_document_view_id,
            &field_operation_id,
        );

        // Create second valid schema field "capacity"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let capacity_field_document_view_id = DocumentViewId::from(relation_operation_id_2);
        let capacity_field_view = create_field(
            "capacity",
            "int",
            &capacity_field_document_view_id,
            &field_operation_id,
        );

        // Create field with invalid DocumentViewId
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let invalid_document_view_id = DocumentViewId::from(invalid_relation_id);
        let field_with_invalid_document_view_id = create_field(
            "capacity",
            "int",
            &invalid_document_view_id,
            &field_operation_id,
        );

        // Passing field with invalid DocumentViewId should fail
        assert!(Schema::from_views(
            schema_view.clone(),
            vec![
                bool_field_view.clone(),
                field_with_invalid_document_view_id.clone()
            ]
        )
        .is_err());

        // Passing too few fields should fail
        assert!(Schema::from_views(schema_view.clone(), vec![bool_field_view.clone()]).is_err());

        // Passing too many fields should fail
        assert!(Schema::from_views(
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
