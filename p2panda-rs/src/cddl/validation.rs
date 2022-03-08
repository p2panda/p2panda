// SPDX-License-Identifier: AGPL-3.0-or-later

use cddl::validator::cbor;

use crate::cddl::CddlValidationError;

/// Checks CBOR bytes against CDDL.
///
/// This method also converts validation errors coming from the `cddl` crate into an
/// concatenated error operation and returns it.
pub fn validate_cbor(cddl: &str, bytes: &[u8]) -> Result<(), CddlValidationError> {
    match cddl::validate_cbor_from_slice(cddl, bytes) {
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

            Err(CddlValidationError::InvalidCBOR(err_str))
        }
        Err(cbor::Error::CBORParsing(_err)) => Err(CddlValidationError::ParsingCBOR),
        Err(cbor::Error::CDDLParsing(err)) => Err(CddlValidationError::ParsingCDDL(err)),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;

    use super::validate_cbor;

    #[test]
    fn validate() {
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
        assert!(validate_cbor(cddl, &cbor_bytes).is_ok());
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
        assert!(validate_cbor(cddl, &cbor_bytes).is_err());
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
        assert!(validate_cbor(cddl, &cbor_bytes).is_err());
    }

    #[test]
    fn invalid_cddl() {
        // cddl definition with an unmatched `{` character
        let cddl = r#"
        panda = {
            name: {
        }
        "#;

        assert!(validate_cbor(cddl, &vec![1u8]).is_err());
    }
}
