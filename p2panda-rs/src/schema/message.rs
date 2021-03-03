/// This schema is used to verify the data integrity of all incoming messages. This does only
/// validate the "meta" message schema and does not check against user data fields as this is part
/// of an additional process called user schema validation.
pub const MESSAGE_SCHEMA: &str = r#"
    message = {
        schema: hash,
        version: 1,
        message-body,
    }

    hash = tstr .regexp "[0-9a-fa-f]{128}"
    message-fields = { + tstr => tstr / int / float / bool }

    ; Create message
    message-body = (
        action: "create",
        fields: message-fields
    )

    ; Update message
    message-body //= (
        action: "update",
        fields: message-fields,
        id: hash,
    )

    ; Delete message
    message-body //= (
        action: "delete",
        id: hash,
    )
"#;
