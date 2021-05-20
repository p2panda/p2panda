/// This schema is used to verify the data integrity of all incoming messages. This does only
/// validate the "meta" message schema and does not check against user data fields as this is part
/// of an additional process called user schema validation.
pub const MESSAGE_SCHEMA: &str = r#"
    message = {
        schema: hash,
        version: 1,
        message-body,
    }

    hash = tstr .regexp "[0-9a-fa-f]{132}"

    ; Create message
    message-body = (
        action: "create", fields: message-fields //
        action: "update", id: hash, fields: message-fields //
        action: "delete", id: hash,
    )

    ; Message fields with key and value
    message-fields = {
        + tstr => {
            message-value-text //
            message-value-integer //
            message-value-float //
            message-value-boolean //
            message-value-relation
        }
    }

    ; Message values
    message-value-text = (
        type: "text",
        value: tstr,
    )

    message-value-integer = (
        type: "integer",
        value: int,
    )

    message-value-float = (
        type: "float",
        value: float,
    )

    message-value-boolean = (
        type: "boolean",
        value: bool,
    )

    message-value-relation = (
        type: "relation",
        value: hash,
    )
"#;
