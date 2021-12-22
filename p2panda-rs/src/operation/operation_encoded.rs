// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use serde::{Deserialize, Serialize};

use crate::hash::Hash;
use crate::operation::{Operation, OperationEncodedError};
#[cfg(not(target_arch = "wasm32"))]
use crate::schema::{validate_schema, MESSAGE_SCHEMA};
use crate::Validate;

/// Operation represented in hex encoded CBOR format.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(
    feature = "db-sqlx",
    derive(sqlx::Type, sqlx::FromRow),
    sqlx(transparent)
)]
pub struct OperationEncoded(String);

impl OperationEncoded {
    /// Validates and wraps encoded operation string into a new `OperationEncoded` instance.
    pub fn new(value: &str) -> Result<OperationEncoded, OperationEncodedError> {
        let inner = Self(value.to_owned());
        inner.validate()?;
        Ok(inner)
    }

    /// Returns the hash of this operation.
    pub fn hash(&self) -> Hash {
        // Unwrap as we already know that the inner value is valid
        Hash::new_from_bytes(self.to_bytes()).unwrap()
    }

    /// Returns encoded operation as string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Decodes hex encoding and returns operation as bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Unwrap as we already know that the inner value is valid
        hex::decode(&self.0).unwrap()
    }

    /// Returns payload size (number of bytes) of encoded operation.
    pub fn size(&self) -> i64 {
        // Divide by 2 as every byte is represented by 2 hex chars.
        self.0.len() as i64 / 2
    }
}

/// Returns an encoded version of this operation.
impl TryFrom<&Operation> for OperationEncoded {
    type Error = OperationEncodedError;

    fn try_from(operation: &Operation) -> Result<Self, Self::Error> {
        // Encode bytes as hex string
        let encoded = hex::encode(&operation.to_cbor());
        OperationEncoded::new(&encoded)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Validate for OperationEncoded {
    type Error = OperationEncodedError;

    /// Checks encoded operation value against hex format and CDDL schema.
    fn validate(&self) -> Result<(), Self::Error> {
        // Validate hex encoding
        let bytes = hex::decode(&self.0).map_err(|_| OperationEncodedError::InvalidHexEncoding)?;

        // Validate CDDL schema
        validate_schema(MESSAGE_SCHEMA, bytes)?;

        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
impl Validate for OperationEncoded {
    type Error = OperationEncodedError;

    /// Checks encoded operation value against hex format.
    ///
    /// Skips CDDL schema validation as this is not supported for wasm targets. See:
    /// https://github.com/anweiss/cddl/issues/83
    fn validate(&self) -> Result<(), Self::Error> {
        hex::decode(&self.0).map_err(|_| OperationEncodedError::InvalidHexEncoding)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use crate::hash::Hash;
    use crate::operation::{AsOperation, Operation, OperationValue};

    use super::OperationEncoded;

    #[test]
    fn validate() {
        // Invalid hex string
        assert!(OperationEncoded::new("123456789Z").is_err());

        // Invalid operation
        assert!(OperationEncoded::new("68656c6c6f2062616d626f6f21").is_err());

        // Valid `create` operation
        assert!(OperationEncoded::new("a466616374696f6e6663726561746566736368656d61784430303230633635353637616533376566656132393365333461396337643133663866326266323364626463336235633762396162343632393331313163343866633738626776657273696f6e01666669656c6473a26b6465736372697074696f6ea26474797065637374726576616c756571666f7220706c6179696e67206368657373646e616d65a26474797065637374726576616c7565656368657373").is_ok());
    }

    #[test]
    fn decode() {
        // This is the operation from `operation.rs` encode and decode test
        let operation_encoded = OperationEncoded::new("a666616374696f6e6675706461746566736368656d61784430303230373865356636653832623436373033393232363638623332623934383634626561376663383032333635326536666533616265373334323234343436663063366776657273696f6e017270726576696f75734f7065726174696f6e73817844303032303464633563306231396265643137666237336431303438363863646663373335656531356439393661393739333565613537376437633964383331366666303362696478443030323036323137646430346666636138396238313938656362356135366539663330323764326437643138303431653363323139346131386134653739393434333635666669656c6473a563616765a2647479706563696e746576616c7565181c66686569676874a2647479706565666c6f61746576616c7565f943006869735f61646d696ea2647479706564626f6f6c6576616c7565f46f70726f66696c655f70696374757265a264747970656872656c6174696f6e6576616c75657844303032306231373765633162663236646662336237303130643437336536643434373133623239623736356239396336653630656362666165373432646534393635343368757365726e616d65a26474797065637374726576616c75656462756275").unwrap();

        let operation = Operation::try_from(&operation_encoded).unwrap();

        assert!(operation.is_update());
        assert!(operation.has_id());
        assert_eq!(
            operation.schema(),
            Hash::new_from_bytes(vec![1, 255, 0]).unwrap()
        );

        let fields = operation.fields().unwrap();

        assert_eq!(
            fields.get("username").unwrap(),
            &OperationValue::Text("bubu".to_owned())
        );
        assert_eq!(fields.get("age").unwrap(), &OperationValue::Integer(28));
        assert_eq!(fields.get("height").unwrap(), &OperationValue::Float(3.5));
        assert_eq!(
            fields.get("is_admin").unwrap(),
            &OperationValue::Boolean(false)
        );
        assert_eq!(
            fields.get("profile_picture").unwrap(),
            &OperationValue::Relation(Hash::new_from_bytes(vec![1, 2, 3]).unwrap())
        );
    }
}
