// SPDX-License-Identifier: AGPL-3.0-or-later

#[cfg(not(target_arch = "wasm32"))]
use cddl::validator::cbor;

use super::SchemaValidationError;

/// Checks CBOR bytes against CDDL schemas.
///
/// This method also converts validation errors coming from the `cddl` crate into an
/// concatenated error operation and returns it.
#[cfg(not(target_arch = "wasm32"))]
pub fn validate_schema(cddl_schema: &str, bytes: Vec<u8>) -> Result<(), SchemaValidationError> {
    match cddl::validate_cbor_from_slice(cddl_schema, &bytes) {
        Err(cbor::Error::Validation(err)) => {
            let err_str = err
                .iter()
                .map(|fe| {
                    format!("{}", fe)
                        // Quotes escaped in error messages from `cddl` crate are actually not unescaped by
                        // format macro.
                        //
                        // See: https://github.com/anweiss/cddl/blob/main/src/validator/cbor.rs#L100
                        .replace('"', "'")
                })
                .collect::<Vec<String>>();

            Err(SchemaValidationError::InvalidSchema(err_str))
        }
        Err(cbor::Error::CBORParsing(_err)) => Err(SchemaValidationError::InvalidCBOR),
        Err(cbor::Error::CDDLParsing(err)) => Err(SchemaValidationError::InvalidCDDL(err)),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use rstest::rstest;

    use crate::{
        operation::OperationEncoded,
        schema::{validation::validate_schema, OPERATION_SCHEMA},
        test_utils::fixtures::operation_encoded,
    };

    #[rstest]
    fn validate_operation_cbor(operation_encoded: OperationEncoded) {
        assert!(validate_schema(OPERATION_SCHEMA, operation_encoded.to_bytes()).is_ok())
    }

    #[test]
    fn validate_cbor() {
        let cddl = r#"
        panda = {
            name: tstr,
            age: int
        }
        "#;

        let value = cbor!({
            "name" => "Latte",
            "age" => 4
        })
        .unwrap();

        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&value, &mut cbor_bytes).unwrap();
        assert!(validate_schema(cddl, cbor_bytes).is_ok());
    }

    #[test]
    fn validate_cbor_error() {
        let cddl = r#"
        panda = {
            name: tstr,
            age: int
        }
        "#;

        let value = cbor!({
            "name" => "Latte",
        })
        .unwrap();

        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&value, &mut cbor_bytes).unwrap();
        assert!(validate_schema(cddl, cbor_bytes).is_err());
    }

    #[test]
    fn invalid_cbor() {
        let cddl = r#"
        panda = {
            name: tstr,
            age: int
        }
        "#;

        let cbor_bytes = Vec::from("}");
        assert!(validate_schema(cddl, cbor_bytes).is_err());
    }

    #[test]
    fn invalid_cddl() {
        let cddl = r#"
        panda = {
            name: {
        }
        "#;

        assert!(validate_schema(cddl, vec![1u8]).is_err());
    }
}
