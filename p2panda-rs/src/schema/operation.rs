// SPDX-License-Identifier: AGPL-3.0-or-later

/// This schema is used to verify the data integrity of all incoming operations. This does only
/// validate the "meta" operation schema and does not check against user data fields as this is part
/// of an additional process called user schema validation.
pub const MESSAGE_SCHEMA: &str = r#"
    operation = {
        schema: hash,
        version: 1,
        operation-body,
    }

    hash = tstr .regexp "[0-9a-f]{68}"

    ; Create operation
    operation-body = (
        action: "create", fields: operation-fields //
        action: "update", id: hash, fields: operation-fields, previousOperations: [1* hash] //
        action: "delete", id: hash, previousOperations: [1* hash]
    )

    ; Operation fields with key and value
    operation-fields = {
        + tstr => {
            operation-value-text //
            operation-value-integer //
            operation-value-float //
            operation-value-boolean //
            operation-value-relation
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
        value: hash,
    )
"#;
