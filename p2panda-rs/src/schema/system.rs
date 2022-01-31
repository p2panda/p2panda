// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::hash::Hash;
use crate::schema::fields::Type;
use crate::schema::SchemaBuilder;

use super::Schema;

// type SchemaStore = BTreeMap<String, Schemas>;

// enum Schemas {
//     Application(Schema),
//     System(SystemSchema)
// }

// enum SystemSchema {
//     KeyGroup(Schema),
//     ApplicationSchema(Schema)
// }


// // Loaded from database + hard coded system schemas

// if (schema_store.valid_operation(operation)) {
//     materialise()
// }

// if (schema_store.valid_query(query: AbstractQuery)) {
//     fetch(query.to_sql())
// }

// schema_store.schemas().map(|schema| todo!() )


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
            action: "create",
            fields: operation-fields-{name}-{hash}
        )
        "#,
        hash = hash,
        name = schema_name
    );

    if can_update {
        schema.push_str(&format!(
            r#"
        operation-body-{name}-{hash} //= (
            action: "update",
            fields: operation-fields-{name}-{hash},
            previousOperations: [1* hash]
        )
        "#,
            hash = hash,
            name = schema_name
        ));
    }

    if can_delete {
        schema.push_str(&format!(
            r#"
        operation-body-{name}-{hash} //= (
            action: "delete",
            previousOperations: [1* hash]
        )
        "#,
            hash = hash,
            name = schema_name
        ));
    }

    // Insert definition for operation fields from schema builder
    schema.push_str(&format!("\n\t{}", &fields_schema_builder));

    schema
}

/// Create CDDL definition for bookmarks schema
fn get_bookmarks_schema() -> Schema {
    let schema_name = "bookmarks";
    let schema_hash = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();

    let fields_label = format!("operation-fields-{}-{}", schema_name, schema_hash.as_str());
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

    let cddl = make_schema_cddl(
        schema_name,
        schema_hash.as_str(),
        &fields_schema,
        true,
        false,
    );
    Schema::new(&schema_hash, &cddl).unwrap()
}

/// Returns global CDDL definition that validates all registered schemas' operations
pub fn get_system_cddl() -> String {
    // (Hash, CDDL) pairs of all known schemas
    let system_schemas: [Schema; 1] = [get_bookmarks_schema()];

    // Concatenated into one string
    let system_schema_cddl = system_schemas
        .iter()
        .map(|schema| format!("{}\n\n", schema))
        .reduce(|cur, acc| format!("{}\n\n{}", acc, cur))
        .unwrap();

    // println!("{}", system_schema_cddl);

    system_schema_cddl
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::get_system_cddl;
    use crate::{
        hash::Hash,
        operation::{Operation, OperationEncoded, OperationFields, OperationValue},
        schema::{Schema, ValidateOperation},
    };

    #[test]
    fn test_validate_correct_system_cddl() {
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
        let prev_ops: Vec<Hash> = vec![Hash::new_from_bytes(vec![1, 2, 3]).unwrap()];
        let operation = Operation::new_update(schema_hash.clone(), prev_ops, fields).unwrap();

        let cddl = get_system_cddl();
        println!("{}", cddl);
        // This instance of `Schema` is not representing a specific schema but the entirety of
        // registered schemas.
        let system_cddl = Schema::new(&schema_hash, &cddl).unwrap();

        let operation_encoded = OperationEncoded::try_from(&operation).unwrap();
        assert!(system_cddl
            .validate_operation(operation_encoded.to_bytes())
            .is_ok());
    }

    #[test]
    fn test_validate_failing_system_cddl() {
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

        let operation = Operation::new_create(schema_hash.clone(), fields).unwrap();

        let cddl = get_system_cddl();
        let operation_encoded = OperationEncoded::try_from(&operation).unwrap();

        let system_cddl = Schema::new(&schema_hash, &cddl).unwrap();
        println!("{}", system_cddl.validate_operation(operation_encoded.to_bytes()).unwrap_err());
        assert!(system_cddl
            .validate_operation(operation_encoded.to_bytes())
            .is_err());
    }
}
