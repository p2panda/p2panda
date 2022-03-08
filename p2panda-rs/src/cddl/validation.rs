// SPDX-License-Identifier: AGPL-3.0-or-later

use cddl_cat::validate_cbor_bytes;

use crate::cddl::CddlValidationError;

/// Checks CBOR bytes against CDDL.
pub fn validate_cbor(cddl: &str, bytes: &[u8]) -> Result<(), CddlValidationError> {
    validate_cbor_bytes("operation", cddl, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;

    use super::validate_cbor;

    #[test]
    fn validate() {
        let cddl = r#"
        operation = {
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
        operation = {
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
        operation = {
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
        operation = {
            name: {
        }
        "#;

        assert!(validate_cbor(cddl, &vec![1u8]).is_err());
    }
}
