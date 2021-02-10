use thiserror::Error;

use crate::atomic::{Hash, Message, Validation};
#[cfg(not(target_arch = "wasm32"))]
use crate::schema::{validate_schema, MESSAGE_SCHEMA};
use crate::Result;

/// Custom error types for `MessageEncoded`.
#[derive(Error, Debug)]
pub enum MessageEncodedError {
    /// Message contains invalid fields.
    #[error("invalid message schema: {0}")]
    InvalidSchema(String),

    /// Encoded message string contains invalid hex characters.
    #[error("invalid hex encoding in message")]
    InvalidHexEncoding,

    /// Message can't be deserialized from invalid CBOR encoding.
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

    /// Returns the decoded version of message.
    pub fn decode(&self) -> Message {
        // Deserialize from CBOR
        serde_cbor::from_slice(&self.to_bytes()).unwrap()
    }

    /// Returns the hash of this message.
    pub fn hash(&self) -> Hash {
        // Unwrap as we already know that the inner value is valid
        Hash::from_bytes(self.to_bytes()).unwrap()
    }

    /// Returns encoded message as string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Decodes hex encoding and returns message as bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Unwrap as we already know that the inner value is valid
        hex::decode(&self.0).unwrap()
    }

    /// Returns payload size (number of bytes) of encoded message.
    pub fn size(&self) -> u64 {
        self.0.len() as u64 / 2
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Validation for MessageEncoded {
    /// Checks encoded message value against hex format and CDDL schema.
    fn validate(&self) -> Result<()> {
        // Validate hex encoding
        let bytes = hex::decode(&self.0).map_err(|_| MessageEncodedError::InvalidHexEncoding)?;

        // Validate CDDL schema
        validate_schema(MESSAGE_SCHEMA, bytes)?;

        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
impl Validation for MessageEncoded {
    /// Checks encoded message value against hex format.
    ///
    /// Skips CDDL schema validation as this is not supported for wasm targets. See:
    /// https://github.com/anweiss/cddl/issues/83
    fn validate(&self) -> Result<()> {
        hex::decode(&self.0).map_err(|_| MessageEncodedError::InvalidHexEncoding)?;
        Ok(())
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
        assert_eq!(message.schema().as_hex(), "00402dc25d32dfb400bb295b663d4706bc47f0cb4f1edff277c737afc8a9232330ae9884fcf6d02141a785c5fd82c196b973e8427efc0c04d0444dcc3059220b9eda");

        let fields = message.fields().unwrap();

        assert_eq!(
            fields.get("username").unwrap(),
            &MessageValue::Text("bubu".to_owned())
        );
        assert_eq!(fields.get("age").unwrap(), &MessageValue::Integer(28));
        assert_eq!(
            fields.get("is_admin").unwrap(),
            &MessageValue::Boolean(false)
        );
    }
}
