// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

use crate::hash::Hash;
use crate::operation::{Operation, OperationEncodedError};
#[cfg(not(target_arch = "wasm32"))]
use crate::schema::{validate_schema, OPERATION_SCHEMA};
use crate::Validate;

/// Operation represented in hex encoded CBOR format.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, StdHash)]
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
    pub fn size(&self) -> u64 {
        // Divide by 2 as every byte is represented by 2 hex chars
        self.0.len() as u64 / 2
    }
}

/// Returns an encoded version of this operation.
impl TryFrom<&Operation> for OperationEncoded {
    type Error = OperationEncodedError;

    fn try_from(operation: &Operation) -> Result<Self, Self::Error> {
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
        validate_schema(OPERATION_SCHEMA, bytes)?;

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

    use rstest::rstest;

    use crate::hash::Hash;
    use crate::operation::{AsOperation, Operation, OperationValue, Relation};
    use crate::test_utils::fixtures::{
        encoded_create_string, fields, random_hash, schema, update_operation,
    };

    use super::OperationEncoded;

    #[rstest]
    fn validate(encoded_create_string: String) {
        // Invalid hex string
        assert!(OperationEncoded::new("123456789Z").is_err());

        // Invalid operation
        assert!(OperationEncoded::new("68656c6c6f2062616d626f6f21").is_err());

        // Valid CREATE operation
        assert!(OperationEncoded::new(&encoded_create_string).is_ok());
    }

    #[rstest]
    fn decode(schema: Hash) {
        // @TODO: Use fixture here
        let operation_encoded = OperationEncoded::new("A566616374696F6E6675706461746566736368656D61784430303230633635353637616533376566656132393365333461396337643133663866326266323364626463336235633762396162343632393331313163343866633738626776657273696F6E017270726576696F75734F7065726174696F6E738178443030323036306138383934383565366533613632613165353665346263333464666136313063393364393136343436316332343963326661326262393662383538653631666669656C6473A563616765A2647479706563696E746576616C7565181C66686569676874A2647479706565666C6F61746576616C7565F943006869735F61646D696EA2647479706564626F6F6C6576616C7565F46F70726F66696C655F70696374757265A264747970656872656C6174696F6E6576616C7565A168646F63756D656E747844303032306231373765633162663236646662336237303130643437336536643434373133623239623736356239396336653630656362666165373432646534393635343368757365726E616D65A26474797065637374726576616C75656462756275").unwrap();

        let operation = Operation::try_from(&operation_encoded).unwrap();

        assert!(operation.is_update());
        assert_eq!(operation.schema(), schema);

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
            &OperationValue::Relation(Relation::new(
                Hash::new_from_bytes(vec![1, 2, 3]).unwrap(),
                Vec::new()
            ))
        );
    }

    #[rstest]
    fn encode_decode_all_field_types(
        schema: Hash,
        #[from(random_hash)] picture_document: Hash,
        #[from(random_hash)] friend_document_1: Hash,
        #[from(random_hash)] friend_document_2: Hash,
        #[from(random_hash)] friend_operation_id: Hash,
        #[with(
            // Schema hash
            schema.clone(),
            // Previous operations
            vec![random_hash()],
            // Operation fields
            fields(vec![
              ("username", OperationValue::Text("bubu".to_owned())),
              ("age", OperationValue::Integer(28)),
              ("height", OperationValue::Float(3.5)),
              ("is_admin", OperationValue::Boolean(false)),
              ("profile_picture", OperationValue::Relation(Relation::new(picture_document.clone(), Vec::new()))),
              ("my_friends", OperationValue::RelationList(vec![
                  Relation::new(friend_document_1.clone(), vec![friend_operation_id.clone()]),
                  Relation::new(friend_document_2.clone(), Vec::new()),
              ])),
            ])
        )]
        update_operation: Operation,
    ) {
        let operation_encoded = OperationEncoded::try_from(&update_operation).unwrap();
        let operation = Operation::try_from(&operation_encoded).unwrap();

        assert!(operation.is_update());
        assert_eq!(operation.schema(), schema);

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
            &OperationValue::Relation(Relation::new(picture_document, Vec::new()))
        );
        assert_eq!(
            fields.get("my_friends").unwrap(),
            &OperationValue::RelationList(vec![
                Relation::new(friend_document_1, vec![friend_operation_id]),
                Relation::new(friend_document_2, vec![])
            ])
        );
    }
}
