// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use serde::{Deserialize, Serialize};

use crate::hash::Hash;
use crate::message::{Message, MessageEncodedError};
#[cfg(not(target_arch = "wasm32"))]
use crate::schema::{validate_schema, MESSAGE_SCHEMA};
use crate::Validate;

/// Message represented in hex encoded CBOR format.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(
    feature = "db-sqlx",
    derive(sqlx::Type, sqlx::FromRow),
    sqlx(transparent)
)]
pub struct MessageEncoded(String);

impl MessageEncoded {
    /// Validates and wraps encoded message string into a new `MessageEncoded` instance.
    pub fn new(value: &str) -> Result<MessageEncoded, MessageEncodedError> {
        let inner = Self(value.to_owned());
        inner.validate()?;
        Ok(inner)
    }

    /// Returns the hash of this message.
    pub fn hash(&self) -> Hash {
        // Unwrap as we already know that the inner value is valid
        Hash::new_from_bytes(self.to_bytes()).unwrap()
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
    pub fn size(&self) -> i64 {
        // Divide by 2 as every byte is represented by 2 hex chars.
        self.0.len() as i64 / 2
    }
}

/// Returns an encoded version of this message.
impl TryFrom<&Message> for MessageEncoded {
    type Error = MessageEncodedError;

    fn try_from(message: &Message) -> Result<Self, Self::Error> {
        // Encode bytes as hex string
        let encoded = hex::encode(&message.to_cbor());
        Ok(MessageEncoded::new(&encoded)?)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Validate for MessageEncoded {
    type Error = MessageEncodedError;

    /// Checks encoded message value against hex format and CDDL schema.
    fn validate(&self) -> Result<(), Self::Error> {
        // Validate hex encoding
        let bytes = hex::decode(&self.0).map_err(|_| MessageEncodedError::InvalidHexEncoding)?;

        // Validate CDDL schema
        validate_schema(MESSAGE_SCHEMA, bytes)?;

        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
impl Validate for MessageEncoded {
    type Error = MessageEncodedError;

    /// Checks encoded message value against hex format.
    ///
    /// Skips CDDL schema validation as this is not supported for wasm targets. See:
    /// https://github.com/anweiss/cddl/issues/83
    fn validate(&self) -> Result<(), Self::Error> {
        hex::decode(&self.0).map_err(|_| MessageEncodedError::InvalidHexEncoding)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use crate::hash::Hash;
    use crate::message::{Message, MessageValue};

    use super::MessageEncoded;

    #[test]
    fn validate() {
        // Invalid hex string
        assert!(MessageEncoded::new("123456789Z").is_err());

        // Invalid message
        assert!(MessageEncoded::new("68656c6c6f2062616d626f6f21").is_err());

        // Valid `delete` message
        assert!(MessageEncoded::new("a466616374696f6e6663726561746566736368656d61784430303230623137376563316266323664666233623730313064343733653664343437313362323962373635623939633665363065636266616537343264653439363534336776657273696f6e02666669656c6473a168757365726e616d65a26474797065637374726576616c75656462756275").is_ok());
    }

    #[test]
    fn decode() {
        // This is the message from `message.rs` encode and decode test
        let message_encoded = MessageEncoded::new("a566616374696f6e6675706461746566736368656d61784430303230373865356636653832623436373033393232363638623332623934383634626561376663383032333635326536666533616265373334323234343436663063366776657273696f6e0262696478443030323036323137646430346666636138396238313938656362356135366539663330323764326437643138303431653363323139346131386134653739393434333635666669656c6473a563616765a2647479706563696e746576616c7565181c66686569676874a2647479706565666c6f61746576616c7565f943006869735f61646d696ea2647479706564626f6f6c6576616c7565f46f70726f66696c655f70696374757265a264747970656872656c6174696f6e6576616c75657844303032306231373765633162663236646662336237303130643437336536643434373133623239623736356239396336653630656362666165373432646534393635343368757365726e616d65a26474797065637374726576616c75656462756275").unwrap();

        let message = Message::try_from(&message_encoded).unwrap();

        assert!(message.is_update());
        assert!(message.has_id());
        assert_eq!(message.schema().clone(), Hash::new_from_bytes(vec![1, 255, 0]).unwrap());

        let fields = message.fields().unwrap();

        assert_eq!(
            fields.get("username").unwrap(),
            &MessageValue::Text("bubu".to_owned())
        );
        assert_eq!(fields.get("age").unwrap(), &MessageValue::Integer(28));
        assert_eq!(fields.get("height").unwrap(), &MessageValue::Float(3.5));
        assert_eq!(
            fields.get("is_admin").unwrap(),
            &MessageValue::Boolean(false)
        );
        assert_eq!(
            fields.get("profile_picture").unwrap(),
            &MessageValue::Relation(Hash::new_from_bytes(vec![1, 2, 3]).unwrap())
        );
    }
}
