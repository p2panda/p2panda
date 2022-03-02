// SPDX-License-Identifier: AGPL-3.0-or-later

/// This CDDL is used to verify the data integrity of all incoming operations.
///
/// This does only validate the general operation format and does not check against application
/// data fields as this is part of an additional process.
pub const OPERATION_FORMAT: &str = r#"
operation = {
    schema: hash,
    version: 1,
    operation-body,
}

hash = tstr .regexp "[0-9a-f]{68}"

relation = hash

pinned_relation = [+ hash];

; Create operation
operation-body = (
    action: "create", fields: operation-fields //
    action: "update", fields: operation-fields, previous_operations: [1* hash] //
    action: "delete", previous_operations: [1* hash]
)

; Operation fields with key and value
operation-fields = {
    + tstr => {
        operation-value-text //
        operation-value-integer //
        operation-value-float //
        operation-value-boolean //
        operation-value-relation //
        operation-value-relation-list
    }
}

; Operation values
operation-value-text = (
    type: "str",
    value: tstr,
)

operation-value-integer = (
    type: "int",
    value: int,
)

operation-value-float = (
    type: "float",
    value: float,
)

operation-value-boolean = (
    type: "bool",
    value: bool,
)

operation-value-relation = (
    type: "relation",
    value: relation / pinned_relation,
)

operation-value-relation-list = (
    type: "relation_list",
    value: [* relation] / [* pinned_relation],
)
"#;

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use ciborium::value::Value;
    use rstest::rstest;

    use crate::cddl::validate_cbor;
    use crate::operation::OperationEncoded;
    use crate::test_utils::fixtures::operation_encoded;

    use super::OPERATION_FORMAT;

    fn to_cbor(value: Value) -> Vec<u8> {
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&value, &mut cbor_bytes).unwrap();
        cbor_bytes
    }

    #[rstest]
    fn valid_operations(operation_encoded: OperationEncoded) {
        assert!(validate_cbor(OPERATION_FORMAT, &operation_encoded.to_bytes()).is_ok());

        assert!(validate_cbor(
            OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "create",
                    "schema" => "0020080f68089c1ad1cef2006a4eec94af5c1e594e4ae1681edb5c458abec67f9457",
                    "version" => 1,
                    "fields" => {
                        "national_dish" => {
                            "value" => "Pumpkin",
                            "type" => "str"
                        },
                        "country" => {
                            "value" => "0020f407359f54a9dbfabba3c5d8cab5fe4e99867dbc81ca1a29588c3bd478712644",
                            "type" => "relation"
                        },
                        "vegan_friendly" => {
                            "value" => true,
                            "type" => "bool"
                        },
                        "yummyness" => {
                            "value" => 8,
                            "type" => "int"
                        },
                        "yumsimumsiness" => {
                            "value" => 7.2,
                            "type" => "float"
                        },
                    },
                })
                .unwrap()
            )
        )
        .is_ok());

        assert!(validate_cbor(
            OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "update",
                    "schema" => "00208432597826bef4ac1c3cb56ba4c79c1b2b656dadbb808d8af46c62dcef6f987d",
                    "version" => 1,
                    "previous_operations" => [
                        "00208f7492d6eb01360a886dac93da88982029484d8c04a0bd2ac0607101b80a6634",
                        "00207134365ce71dca6bd7c31d04bfb3244b29897ab538906216fc8ff3d6189410ad",
                    ],
                    "fields" => {
                        "national_dish" => {
                            "value" => "Almonds",
                            "type" => "str"
                        },
                    },
                })
                .unwrap()
            )
        )
        .is_ok());

        assert!(validate_cbor(
            OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "delete",
                    "schema" => "002094734a821e9987876a30e6040191baea92702ce3e18342032fde6e54b0f63fd0",
                    "version" => 1,
                    "previous_operations" => [
                        "00203ea9940af9e5a191a81a49a118ee049283c3f62e879b33f879e154abad3e682f",
                    ],
                })
                .unwrap()
            )
        )
        .is_ok());
    }

    #[test]
    fn invalid_operations() {
        assert!(validate_cbor(
            OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "create",
                    // Hash invalid (64 instead of 68 characters)
                    "schema" => "80f68089c1ad1cef2006a4eec94af5c1e594e4ae1681edb5c458abec67f9457",
                    "version" => 1,
                    "fields" => {
                        "food" => {
                            "value" => "Pumkin",
                            "type" => "str"
                        }
                    }
                })
                .unwrap()
            )
        )
        .is_err());

        assert!(validate_cbor(
            OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    // Fields missing in UPDATE operation
                    "action" => "update",
                    "schema" => "002080f68089c1ad1cef2006a4eec94af5c1e594e4ae1681edb5c458abec67f9457",
                    "version" => 1,
                    "previous_operations" => [
                        "002062b773e62f48cdbbfd3e24956cffd3a9ccb0a844917f1cb726f17405b5e9e2ca",
                        "002061c6e4d915481b00318ca44196410788a740d0354ab30c5fb5bb387d689b69e7",
                    ],
                })
                .unwrap()
            )
        )
        .is_err());

        assert!(validate_cbor(
            OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    // Previous operations missing in DELETE operation
                    "action" => "delete",
                    "schema" => "002080f68089c1ad1cef2006a4eec94af5c1e594e4ae1681edb5c458abec67f9457",
                    "version" => 1,
                })
                .unwrap()
            )
        )
        .is_err());

        assert!(validate_cbor(
            OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "create",
                    "schema" => "002080f68089c1ad1cef2006a4eec94af5c1e594e4ae1681edb5c458abec67f9457",
                    "version" => 1,
                    "fields" => {
                        "size" => {
                            // Value and type do not match
                            "value" => "This is not a number",
                            "type" => "int",
                        },
                    },
                })
                .unwrap()
            )
        )
        .is_err());

        assert!(validate_cbor(
            OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    // Version missing
                    "action" => "delete",
                    "schema" => "0020687af9bd717de34ac24ce601116a3b5dabc396eabaf92c2da8010b5703dc4612",
                    "previous_operations" => [
                        "00201b9ce32f4783941109210d349558baa9cf9216411201c848394379ef5bbc85b2",
                    ],
                })
                .unwrap()
            )
        )
        .is_err());

        assert!(validate_cbor(
            OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "delete",
                    "schema" => "0020687af9bd717de34ac24ce601116a3b5dabc396eabaf92c2da8010b5703dc4612",
                    "version" => 1,
                    // Huch!
                    "racoon" => "Bwaahaha!",
                    "previous_operations" => [
                        "00201b9ce32f4783941109210d349558baa9cf9216411201c848394379ef5bbc85b2",
                    ],
                })
                .unwrap()
            )
        )
        .is_err());
    }
}
