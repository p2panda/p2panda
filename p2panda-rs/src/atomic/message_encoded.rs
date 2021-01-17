use anyhow::bail;
use cddl::validator::cbor;
use thiserror::Error;

use crate::atomic::{Validation, Message};
use crate::error::Result;

/// Concise Data Definition Language (CDDL) Schema of p2panda messages. See:
/// https://tools.ietf.org/html/rfc8610
///
/// This schema is used to verify the data integrity of all incoming messages. This does only
/// validate the "meta" message schema and does not check against user data fields as this is part
/// of an additional process called user schema validation.
///
/// @TODO: Fix issue with schema:
/// This schema accepts maps as values in `message-fields` even though it should only accept
/// `tstr`. See: https://github.com/anweiss/cddl/issues/82
const MESSAGE_SCHEMA: &str = r#"
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

#[derive(Error, Debug)]
pub enum MessageEncodedError {
    #[error("invalid message schema: {0}")]
    InvalidSchema(String),

    #[error("invalid hex encoding in message")]
    InvalidHexEncoding,

    #[error("invalid CBOR format")]
    InvalidCBOR,
}

/// Message represented in hex encoded CBOR format.
#[derive(Clone, Debug)]
pub struct MessageEncoded(String);

impl MessageEncoded {
    /// Validates and returns a new encoded message instance.
    pub fn new(value: &str) -> Result<MessageEncoded> {
        let inner = Self(value.to_owned());
        inner.validate()?;
        Ok(inner)
    }

    /// Returns encoded message as string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Decodes hex encoding and returns message as bytes.
    pub fn as_bytes(&self) -> Vec<u8> {
        // Unwrap as we already know that the inner value is valid
        hex::decode(&self.0).unwrap()
    }

    /// Returns the decoded version of this message.
    pub fn decode(&self) -> Message {
        // Deserialize from CBOR
        serde_cbor::from_slice(&self.as_bytes()).unwrap()
    }
}

impl Validation for MessageEncoded {
    /// Checks encoded message value against hex format and CDDL schema.
    ///
    /// This helper method also converts validation errors coming from the cddl crate into an
    /// concatenated error message and returns it.
    fn validate(&self) -> Result<()> {
        let bytes = hex::decode(&self.0).map_err(|_| MessageEncodedError::InvalidHexEncoding)?;

        match cddl::validate_cbor_from_slice(MESSAGE_SCHEMA, &bytes) {
            Err(cbor::Error::Validation(err)) => {
                let err_str = err
                    .iter()
                    .map(|fe| format!("{}: \"{}\"", fe.cbor_location, fe.reason))
                    .collect::<Vec<String>>()
                    .join(", ");

                bail!(MessageEncodedError::InvalidSchema(err_str))
            }
            Err(_) => {
                bail!(MessageEncodedError::InvalidCBOR)
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MessageEncoded;

    use crate::atomic::MessageValue;

    #[test]
    fn validate() {
        // Invalid hex string
        assert!(MessageEncoded::new("123456789Z").is_err());

        // Invalid message
        assert!(MessageEncoded::new("68656c6c6f2062616d626f6f21").is_err());

        // Valid `delete` message
        assert!(MessageEncoded::new("a466616374696f6e6664656c65746566736368656d6178843030343032646332356433326466623430306262323935623636336434373036626334376630636234663165646666323737633733376166633861393233323333306165393838346663663664303231343161373835633566643832633139366239373365383432376566633063303464303434346463633330353932323062396564616776657273696f6e016269647884303034306366393466366436303536353765393063353433623063393139303730636461616637323039633565316561353861636238663335363866613231313432363864633961633362616665313261663237376432383666636537646335396237633063333438393733633465396461636265373934383565353661633261373032").is_ok());
    }

    #[test]
    fn decode() {
        let message_encoded = MessageEncoded::new("a466616374696f6e6663726561746566736368656d6178843030343032646332356433326466623430306262323935623636336434373036626334376630636234663165646666323737633733376166633861393233323333306165393838346663663664303231343161373835633566643832633139366239373365383432376566633063303464303434346463633330353932323062396564616776657273696f6e01666669656c6473a363616765181c68757365726e616d6564627562756869735f61646d696ef4").unwrap();

        let message = message_encoded.decode();

        assert!(message.is_create());
        assert!(!message.has_id());
        assert_eq!(message.schema().to_hex(), "00402dc25d32dfb400bb295b663d4706bc47f0cb4f1edff277c737afc8a9232330ae9884fcf6d02141a785c5fd82c196b973e8427efc0c04d0444dcc3059220b9eda");

        let fields = message.fields().unwrap();

        assert_eq!(fields.get("username").unwrap(), &MessageValue::Text("bubu".to_owned()));
        assert_eq!(fields.get("age").unwrap(), &MessageValue::Integer(28));
        assert_eq!(fields.get("is_admin").unwrap(), &MessageValue::Boolean(false));
    }
}
