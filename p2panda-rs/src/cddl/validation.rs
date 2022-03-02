// SPDX-License-Identifier: AGPL-3.0-or-later

use cddl::validator::cbor;

use crate::cddl::CDDLValidationError;

/// Checks CBOR bytes against CDDL.
///
/// This method also converts validation errors coming from the `cddl` crate into an
/// concatenated error operation and returns it.
pub fn validate_cddl(cddl_schema: &str, bytes: Vec<u8>) -> Result<(), CDDLValidationError> {
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

            Err(CDDLValidationError::InvalidSchema(err_str))
        }
        Err(cbor::Error::CBORParsing(_err)) => Err(CDDLValidationError::InvalidCBOR),
        Err(cbor::Error::CDDLParsing(err)) => Err(CDDLValidationError::InvalidCDDL(err)),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use rstest::rstest;

    use crate::cddl::OPERATION_FORMAT;
    use crate::operation::OperationEncoded;
    use crate::test_utils::fixtures::operation_encoded;

    use super::validate_cddl;

    #[rstest]
    fn validate_operation_cbor(operation_encoded: OperationEncoded) {
        assert!(validate_cddl(OPERATION_FORMAT, operation_encoded.to_bytes()).is_ok())
    }

    #[test]
    fn validate_cbor() {
        let cddl = r#"
        panda = {
            name: tstr,
            age: int
        }
        "#;

        let age: usize = 4;

        let value = cbor!({
            "name" => "Latte",
            "age" => age,
        })
        .unwrap();

        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&value, &mut cbor_bytes).unwrap();
        assert!(validate_cddl(cddl, cbor_bytes).is_ok());
    }

    #[test]
    fn validate_cbor_error() {
        let cddl = r#"
        panda = {
            name: tstr,
            age: int
        }
        "#;

        // value missing `age`
        let value = cbor!({
            "name" => "Latte",
        })
        .unwrap();

        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&value, &mut cbor_bytes).unwrap();
        assert!(validate_cddl(cddl, cbor_bytes).is_err());
    }

    #[test]
    fn invalid_cbor() {
        let cddl = r#"
        panda = {
            name: tstr,
            age: int
        }
        "#;

        // Invalid CBOR
        let cbor_bytes = Vec::from("}");
        assert!(validate_cddl(cddl, cbor_bytes).is_err());
    }

    #[test]
    fn invalid_cddl() {
        // cddl definition with an unmatched `{` character
        let cddl = r#"
        panda = {
            name: {
        }
        "#;

        assert!(validate_cddl(cddl, vec![1u8]).is_err());
    }
}
