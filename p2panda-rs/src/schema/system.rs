// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::hash::Hash;
use crate::schema::fields::Type;
use crate::schema::SchemaBuilder;

/// Make CDDL definition for operations from a given schema
fn make_schema_cddl(
    schema_name: &str,
    hash: &str,
    fields_schema_builder: &SchemaBuilder,
    can_update: bool,
    can_delete: bool,
) -> String {
    let mut schema = format!(
        r#"; System schema {name}

        {name} = {{
            schema: "{hash}",
            version: 1,
            operation-body-{name}-{hash},
        }}

        hash = tstr .regexp "[0-9a-f]{{68}}"

        operation-body-{name}-{hash} = (
            action: "create", fields: operation-fields-{name}-{hash}"#,
        hash = hash,
        name = schema_name
    );

    if can_update {
        schema.push_str(&format!(
            r#" //
        action: "update", fields: operation-fields-{name}-{hash}, previousOperations: [1* hash]"#,
            hash = hash,
            name = schema_name
        ));
    }

    if can_delete {
        schema.push_str(
            r#" //
        action: "delete", previousOperations: [1* hash]"#,
        );
    }

    // Close operation-body definition
    schema.push_str("\n\t)\n");

    // Insert definition for operation fields from schema builder
    schema.push_str(&format!("{}", &fields_schema_builder));

    schema
}

/// Create CDDL definition for bookmarks schema
fn get_bookmarks_schema() -> (String, String) {
    let schema_name = "bookmarks";
    let schema_hash: String = Hash::new_from_bytes(vec![1, 2, 3]).unwrap().as_str().into();

    let fields_label = format!("operation-fields-{}-{}", schema_name, schema_hash);
    let mut fields_schema = SchemaBuilder::new(fields_label);
    fields_schema
        .add_operation_field("title".into(), Type::Tstr)
        .unwrap();
    fields_schema
        .add_operation_field("url".into(), Type::Tstr)
        .unwrap();
    fields_schema
        .add_operation_field("created".into(), Type::Tstr)
        .unwrap();

    let cddl = make_schema_cddl(schema_name, &schema_hash, &fields_schema, true, true);

    (schema_hash, cddl)
}

/// Returns global CDDL definition that validates all registered schemas' operations
pub fn get_system_cddl() -> String {
    // (Hash, CDDL) pairs of all known schemas
    let system_schemas: [(String, String); 1] = [get_bookmarks_schema()];

    // Concatenated into one string
    let system_schema_cddl = system_schemas
        .iter()
        .map(|(_, schema)| format!("{}\n\n", schema))
        .reduce(|cur, acc| format!("{}\n\n{}", acc, cur))
        .unwrap();

    system_schema_cddl
}

#[cfg(test)]
mod tests {
    use super::get_system_cddl;
    use crate::{
        hash::Hash,
        operation::{Operation, OperationEncoded, OperationFields, OperationValue},
    };

    #[test]
    fn test_validate_with_system_cddl() {
        use std::convert::TryFrom;

        let schema_hash = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();

        // Create test operation
        let mut fields = OperationFields::new();
        fields
            .add("title", OperationValue::Text("p2panda".into()))
            .unwrap();
        fields
            .add("url", OperationValue::Text("https://p2panda.org".into()))
            .unwrap();
        fields
            .add("created", OperationValue::Text("2022-01-31".into()))
            .unwrap();

        let operation = Operation::new_create(schema_hash, fields).unwrap();

        let cddl = get_system_cddl();
        let operation_encoded = OperationEncoded::try_from(&operation).unwrap();
        cddl::validate_cbor_from_slice(&cddl, &operation_encoded.to_bytes()).unwrap();
    }
}
