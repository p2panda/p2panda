use validator::{Validate, ValidationError, ValidationErrors};

use crate::error::{Result, ValidationResult};

// CDDL Schema
const MESSAGE_SCHEMA: &str = r#"
    message = {
        schema: entry-hash,
        version: 1,
        message-body,
    }

    entry-hash = tstr .regexp "[0-9a-fa-f]{128}"
    message-fields = (+ tstr => any)

    ; Create message
    message-body = (
        action: "create",
        fields: message-fields
    )

    ; Update message
    message-body //= (
        action: "update",
        fields: message-fields,
        id: entry-hash,
    )

    ; Delete message
    message-body //= (
        action: "delete",
        id: entry-hash,
    )
"#;

/// Message represented in hex encoded CBOR format.
#[derive(Clone, Debug)]
pub struct MessageEncoded(String);

impl MessageEncoded {
    /// Validates and returns an encoded message instance when correct.
    pub fn new(value: &str) -> Result<Self> {
        let message_encoded = Self(String::from(value));
        message_encoded.validate()?;
        Ok(message_encoded)
    }
}

impl Validate for MessageEncoded {
    fn validate(&self) -> ValidationResult {
        let mut errors = ValidationErrors::new();

        // Check if message is hex encoded
        match hex::decode(self.0.to_owned()) {
            Ok(bytes) => {
                match cddl::validate_cbor_from_slice(MESSAGE_SCHEMA, &bytes) {
                    Err(err) => {
                        errors.add(
                            "encoded_message",
                            ValidationError::new("invalid message schema"),
                        );
                        eprintln!("{}", err);
                    }
                    _ => {},
                }
            }
            Err(_) => {
                errors.add(
                    "encoded_message",
                    ValidationError::new("invalid hex string"),
                );
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MessageEncoded;

    #[test]
    fn validate() {
        // Invalid hex string
        assert!(MessageEncoded::new("123456789Z").is_err());

        // Invalid CBOR
        assert!(MessageEncoded::new("68656c6c6f2062616d626f6f21").is_err());
    }
}
