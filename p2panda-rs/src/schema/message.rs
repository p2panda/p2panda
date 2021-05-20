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
    message-body //= (
        action: "create",
        fields: message-fields
    )

    message-body //= (
        action: "update",
        id: hash,
        fields: message-fields
    )

    message-body //= (
        action: "delete",
        id: hash,
    )

    ; Message fields with key and value
    message-fields = { + tstr => { message-value } }

    ; Message values
    message-value //= (
        type: "text",
        value: tstr,
    )

    message-value //= (
        type: "integer",
        value: int,
    )

    message-value //= (
        type: "float",
        value: float,
    )

    message-value //= (
        type: "boolean",
        value: bool,
    )

    message-value //= (
        type: "relation",
        value: hash,
    )
"#;
