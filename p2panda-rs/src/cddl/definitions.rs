// SPDX-License-Identifier: AGPL-3.0-or-later

use lazy_static::lazy_static;

const CDDL_HEADER: &str = r#"
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; p2panda Operation Header v1
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; Header file with the following undefined fields
; which need to be specified in additional cddl:
;
; - schema_id
; - create_fields
; - update_fields
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

>>>>>>> Add cddl definitions for schema_v1 and schema_field_v1:p2panda-rs/src/cddl/definitions.rs
operation = {
    version: 1,
    schema: schema_id,
    operation_body,
}

; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; Core types
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

entry_hash = tstr .regexp "[0-9a-f]{68}"

previous_operations = [+ entry_hash]

relation = entry_hash
pinned_relation = [+ entry_hash]
relation_list = [* relation]
pinned_relation_list = [* pinned_relation]

; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; Operation body
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

operation_body = (
    action: "create", fields: create_fields //
    action: "update", fields: update_fields, previous_operations: previous_operations //
    action: "delete", previous_operations: previous_operations
)

; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; Operation values
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

value_text = (
    type: "str",
    value: tstr,
)

value_integer = (
    type: "int",
    value: int,
)

value_float = (
    type: "float",
    value: float,
)

value_boolean = (
    type: "bool",
    value: bool,
)

value_relation = (
    type: "relation",
    value: relation / pinned_relation,
)

value_relation_list = (
    type: "relation_list",
    value: relation_list / pinned_relation_list,
)
"#;

const CDDL_ANY_OPERATION: &str = r#"
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; p2panda Operation Body v1
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

schema_id = "schema_v1" / "schema_field_v1" / entry_hash

create_fields = fields

update_fields = fields

; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; Fields
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

fields = {
    + tstr => {
        value_text //
        value_integer //
        value_float //
        value_boolean //
        value_relation //
        value_relation-list
    }
}
"#;

const CDDL_SCHEMA_V1: &str = r#"
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; System Schema "Schema" v1
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

schema_id = "schema_v1"

create_fields = { name, description, fields }

update_fields = { + (name // description // fields) }

; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; Fields
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

name = (
    name: { value_text },
)

description = (
    description: { value_text },
)

fields = (
    fields: {
        type: "relation_list",
        value: pinned_relation_list,
    },
)
"#;

const CDDL_SCHEMA_FIELD_V1: &str = r#"
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; System Schema "Schema field" v1
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

schema_id = "schema_field_v1"

create_fields = { name, description, field_type }

update_fields = { + (name // description // field_type) }

; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; Fields
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

name = (
    name: { value_text },
)

description = (
    description: { value_text },
)

field_type = (
    field_type: {
        type: "str",
        value: "str" / "int" / "float" / "bool" / "relation" / "relation_list",
    }
)
"#;

/// This CDDL is used to verify the format of _all_ incoming operations.
///
/// This does only validate the "general" operation schema and does not check against application
/// data fields as this is part of an additional process called application schema validation.
pub fn operation_format() -> &'static str {
    lazy_static! {
        static ref OPERATION_FORMAT: String = CDDL_HEADER.to_owned() + CDDL_ANY_OPERATION;
    }
    &OPERATION_FORMAT
}

/// CDDL definition of "schema_v1" system operations.
pub fn schema_v1_format() -> &'static str {
    lazy_static! {
        static ref SCHEMA_V1_FORMAT: String = CDDL_HEADER.to_owned() + CDDL_SCHEMA_V1;
    }
    &SCHEMA_V1_FORMAT
}

/// CDDL definition of "schema_field_v1" system operations.
pub fn schema_field_v1_format() -> &'static str {
    lazy_static! {
        static ref SCHEMA_FIELD_V1_FORMAT: String = CDDL_HEADER.to_owned() + CDDL_SCHEMA_FIELD_V1;
    }
    &SCHEMA_FIELD_V1_FORMAT
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use ciborium::value::Value;
    use rstest::rstest;

    use crate::cddl::validate_cbor;
    use crate::operation::OperationEncoded;
    use crate::test_utils::fixtures::operation_encoded;

    use super::{operation_format, schema_field_v1_format, schema_v1_format};

    fn to_cbor(value: Value) -> Vec<u8> {
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&value, &mut cbor_bytes).unwrap();
        cbor_bytes
    }

    #[rstest]
    fn valid_operations(operation_encoded: OperationEncoded) {
        assert!(validate_cbor(operation_format(), &operation_encoded.to_bytes()).is_ok());

        assert!(validate_cbor(
            operation_format(),
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
        ).is_ok());

        assert!(validate_cbor(
            operation_format(),
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
            operation_format(),
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
            operation_format(),
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
            operation_format(),
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
            operation_format(),
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
            operation_format(),
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
            operation_format(),
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
            operation_format(),
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

    #[test]
    fn valid_schema_v1() {
        assert!(validate_cbor(
            schema_v1_format(),
            &to_cbor(
                cbor!({
                    "action" => "create",
                    "schema" => "schema_v1",
                    "version" => 1,
                    "fields" => {
                        "name" => {
                            "value" => "Locations",
                            "type" => "str"
                        },
                        "description" => {
                            "value" => "Holds information about places",
                            "type" => "str"
                        },
                        "fields" => {
                            "value" => [
                                [
                                    "0020c039b78e3f9a84370e23642d911d2648f9db0b9150e43c853de863936bdefe5d",
                                    "0020981f3763e1cefab859c315157b79179188f8187da4d53eea3fb8a571a3b5c0a6",
                                ],
                                [
                                    "00206a98fffb0b1424ada1ed241b32da8287852d6b4eb37a1b381892c4fbd800e9e8",
                                ],
                            ],
                            "type" => "relation_list"
                        },
                    },
                })
                .unwrap()
            )
        ).is_ok());

        assert!(validate_cbor(
            schema_v1_format(),
            &to_cbor(
                cbor!({
                    "action" => "update",
                    "schema" => "schema_v1",
                    "version" => 1,
                    "previous_operations" => [
                        "00207134365ce71dca6bd7c31d04bfb3244b29897ab538906216fc8ff3d6189410ad",
                    ],
                    "fields" => {
                        "name" => {
                            "value" => "Telephones",
                            "type" => "str"
                        },
                    },
                })
                .unwrap()
            )
        )
        .is_ok());

        assert!(validate_cbor(
            schema_v1_format(),
            &to_cbor(
                cbor!({
                    "action" => "delete",
                    "schema" => "schema_v1",
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
    fn valid_schema_field_v1() {
        assert!(validate_cbor(
            schema_field_v1_format(),
            &to_cbor(
                cbor!({
                    "action" => "create",
                    "schema" => "schema_field_v1",
                    "version" => 1,
                    "fields" => {
                        "name" => {
                            "value" => "Size",
                            "type" => "str"
                        },
                        "description" => {
                            "value" => "In centimeters",
                            "type" => "str"
                        },
                        "field_type" => {
                            "value" => "float",
                            "type" => "str"
                        },
                    },
                })
                .unwrap()
            )
        )
        .is_ok());

        assert!(validate_cbor(
            schema_field_v1_format(),
            &to_cbor(
                cbor!({
                    "action" => "update",
                    "schema" => "schema_field_v1",
                    "version" => 1,
                    "previous_operations" => [
                        "00208a5cbba0facc96f22fe3c283e05706c74801282bb7ba315fb5c77caa44689846",
                        "0020e967334f97ac477bf1f53568e475376ae28687e272de3f3d0672ec6f2aa9be53",
                    ],
                    "fields" => {
                        "field_type" => {
                            "value" => "relation_list",
                            "type" => "str"
                        },
                    },
                })
                .unwrap()
            )
        )
        .is_ok());

        assert!(validate_cbor(
            schema_field_v1_format(),
            &to_cbor(
                cbor!({
                    "action" => "delete",
                    "schema" => "schema_field_v1",
                    "version" => 1,
                    "previous_operations" => [
                        "002066f3cec300b76993da433f80c0c32104678e483fa24d59625d0e3994c09115e2",
                    ],
                })
                .unwrap()
            )
        )
        .is_ok());
    }
}
