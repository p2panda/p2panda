// SPDX-License-Identifier: AGPL-3.0-or-later

/// This schema is used to verify the data integrity of all incoming operations.
///
/// This does only validate the "meta" operation schema and does not check against application data
/// fields as this is part of an additional process called application schema validation.
pub const OPERATION_SCHEMA: &str = r#"
    operation = {
        schema: hash,
        version: 1,
        operation-body,
    }

    hash = tstr .regexp "[0-9a-f]{68}"

    relation = hash

    pinned_relation = [+ hash];

    ; Create operation
    operation-body = (
        action: "create", fields: operation-fields //
        action: "update", fields: operation-fields, previous_operations: [1* hash] //
        action: "delete", previous_operations: [1* hash]
    )

    ; Operation fields with key and value
    operation-fields = {
        + tstr => {
            operation-value-text //
            operation-value-integer //
            operation-value-float //
            operation-value-boolean //
            operation-value-relation //
            operation-value-relation-list
        }
    }

    ; Operation values
    operation-value-text = (
        type: "str",
        value: tstr,
    )

    operation-value-integer = (
        type: "int",
        value: int,
    )

    operation-value-float = (
        type: "float",
        value: float,
    )

    operation-value-boolean = (
        type: "bool",
        value: bool,
    )

    operation-value-relation = (
        type: "relation",
        value: relation / pinned_relation,
    )

    operation-value-relation-list = (
        type: "relation_list",
        value: [* relation] / [* pinned_relation],
    )
"#;
