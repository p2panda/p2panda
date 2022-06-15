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
)"#;

const CDDL_ANY_OPERATION: &str = r#"
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; p2panda Operation Body v1
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

; Application schema ids consist of sections separated by an underscore.
; The first section is the name, which has 1-64 characters, must start
; with a letter and must contain only alphanumeric characters and
; underscores. The remaining sections are the document view id of the
; schema's `schema_definition_v1` document, represented as alphabetically
; sorted hex-encoded operation ids, separated by underscores.
application_schema_id = tstr .regexp "[A-Za-z]{1}[A-Za-z0-9_]{0,63}_([0-9A-Za-z]{68})(_[0-9A-Za-z]{68})*"

system_schema_id = "schema_definition_v1" / "schema_field_definition_v1"

schema_id =  system_schema_id / application_schema_id

create_fields = fields

update_fields = fields

; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; Fields
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

fields = {
    + tstr =>
        tstr /
        int /
        float /
        bool /
        relation /
        relation_list /
        pinned_relation /
        pinned_relation_list
}
"#;

const CDDL_SCHEMA_V1: &str = r#"
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; System Schema "Schema" v1
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

schema_id = "schema_definition_v1"

create_fields = { name, description, fields }

update_fields = { + (name // description // fields) }

; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; Fields
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

name = (
    name: tstr,
)

description = (
    description: tstr,
)

fields = (
    fields: pinned_relation_list,
)
"#;

const CDDL_SCHEMA_FIELD_V1: &str = r#"
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; System Schema "Schema field" v1
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

schema_id = "schema_field_definition_v1"

create_fields = { name, description, field_type }

update_fields = { + (name // description // field_type) }

; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
; Fields
; ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

name = (
    name: tstr,
)

description = (
    description: tstr,
)

field_type = (
    field_type: "str" / "int" / "float" / "bool" / "relation" /
    "relation_list" / "pinned_relation" / "pinned_relation_list"
)
"#;

lazy_static! {
    /// This CDDL is used to verify the format of _all_ incoming operations.
    ///
    /// This does only validate the "general" operation schema and does not check against application
    /// data fields as this is part of an additional process called application schema validation.
    pub static ref OPERATION_FORMAT: String = {
        format!("{}{}", CDDL_HEADER, CDDL_ANY_OPERATION)
    };

    /// CDDL definition of "schema_definition_v1" system operations.
    pub static ref SCHEMA_V1_FORMAT: String = {
        format!("{}{}", CDDL_HEADER, CDDL_SCHEMA_V1)
    };

    /// CDDL definition of "schema_field_definition_v1" system operations.
    pub static ref SCHEMA_FIELD_V1_FORMAT: String = {
        format!("{}{}", CDDL_HEADER, CDDL_SCHEMA_FIELD_V1)
    };
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use ciborium::value::Value;
    use rstest::rstest;

    use crate::cddl::validate_cbor;
    use crate::operation::OperationEncoded;
    use crate::test_utils::fixtures::operation_encoded;

    use super::{OPERATION_FORMAT, SCHEMA_FIELD_V1_FORMAT, SCHEMA_V1_FORMAT};

    fn to_cbor(value: Value) -> Vec<u8> {
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&value, &mut cbor_bytes).unwrap();
        cbor_bytes
    }

    #[rstest]
    fn valid_operations(operation_encoded: OperationEncoded) {
        assert!(validate_cbor(&OPERATION_FORMAT, &operation_encoded.to_bytes()).is_ok());

        assert!(validate_cbor(
            &OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "create",
                    "schema" => "menu_0020080f68089c1ad1cef2006a4eec94af5c1e594e4ae1681edb5c458abec67f9457",
                    "version" => 1,
                    "fields" => {
                        "national_dish" => "Pumpkin",
                        "country" => "0020f407359f54a9dbfabba3c5d8cab5fe4e99867dbc81ca1a29588c3bd478712644",
                        "vegan_friendly" => true,
                        "yummyness" => 8,
                        "yumsimumsiness" => 7.2,
                    },
                })
                .unwrap()
            )
        ).is_ok());

        assert!(validate_cbor(
            &OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "update",
                    "schema" => "menu_00208432597826bef4ac1c3cb56ba4c79c1b2b656dadbb808d8af46c62dcef6f987d",
                    "version" => 1,
                    "previous_operations" => [
                        "00208f7492d6eb01360a886dac93da88982029484d8c04a0bd2ac0607101b80a6634",
                        "00207134365ce71dca6bd7c31d04bfb3244b29897ab538906216fc8ff3d6189410ad",
                    ],
                    "fields" => {
                        "national_dish" => "Almonds",
                    },
                })
                .unwrap()
            )
        )
        .is_ok());

        assert!(validate_cbor(
            &OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "delete",
                    "schema" => "menu_002094734a821e9987876a30e6040191baea92702ce3e18342032fde6e54b0f63fd0",
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
            &OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "create",
                    // Hash invalid (64 instead of 68 characters)
                    "schema" => "menu_80f68089c1ad1cef2006a4eec94af5c1e594e4ae1681edb5c458abec67f",
                    "version" => 1,
                    "fields" => {
                        "food" => "Pumpkin"
                    }
                })
                .unwrap()
            )
        )
        .is_err());

        assert!(validate_cbor(
            &OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    // Fields missing in UPDATE operation
                    "action" => "update",
                    "schema" => "menu_80f68089c1ad1cef2006a4eec94af5c1e594e4ae1681edb5c458abec67f9457",
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
            &OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    // Previous operations missing in DELETE operation
                    "action" => "delete",
                    "schema" => "menu_80f68089c1ad1cef2006a4eec94af5c1e594e4ae1681edb5c458abec67f9457",
                    "version" => 1,
                })
                .unwrap()
            )
        )
        .is_err());

        assert!(validate_cbor(
            &OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "create",
                    "schema" => "menu_80f68089c1ad1cef2006a4eec94af5c1e594e4ae1681edb5c458abec67f9457",
                    "version" => 1,
                    "fields" => {
                        "size" => "This is not a number",
                    },
                })
                .unwrap()
            )
        )
        .is_err());

        assert!(validate_cbor(
            &OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    // Version missing
                    "action" => "delete",
                    "schema" => ["0020687af9bd717de34ac24ce601116a3b5dabc396eabaf92c2da8010b5703dc4612"],
                    "previous_operations" => [
                        "00201b9ce32f4783941109210d349558baa9cf9216411201c848394379ef5bbc85b2",
                    ],
                })
                .unwrap()
            )
        )
        .is_err());

        assert!(validate_cbor(
            &OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "delete",
                    "schema" => ["0020458710a538d2f11b811ba6db8851e52323916e906cdd695cc2d4f4e77d35b2a2"],
                    "version" => 1,
                    "previous_operations" => [
                        "0020b39b995e4f9d782a51d9afbc8260e5802b3a13920beb3d09e787ccfc74176c26",
                        // This is not a hash
                        "Yes, Indeed, this is not a hash! https://vimeo.com/559636244",
                    ],
                })
                .unwrap()
            )
        )
        .is_err());

        assert!(validate_cbor(
            &OPERATION_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "delete",
                    "schema" => ["0020687af9bd717de34ac24ce601116a3b5dabc396eabaf92c2da8010b5703dc4612"],
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
    fn valid_schema_definition_v1() {
        assert!(validate_cbor(
            &SCHEMA_V1_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "create",
                    "schema" => "schema_definition_v1",
                    "version" => 1,
                    "fields" => {
                        "name" => "Locations",
                        "description" => "Holds information about places",
                        "fields" => [
                            [
                                "0020c039b78e3f9a84370e23642d911d2648f9db0b9150e43c853de863936bdefe5d",
                                "0020981f3763e1cefab859c315157b79179188f8187da4d53eea3fb8a571a3b5c0a6",
                            ],
                            [
                                "00206a98fffb0b1424ada1ed241b32da8287852d6b4eb37a1b381892c4fbd800e9e8",
                            ],
                        ],
                },
                })
                .unwrap()
            )
        ).is_ok());

        assert!(validate_cbor(
            &SCHEMA_V1_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "update",
                    "schema" => "schema_definition_v1",
                    "version" => 1,
                    "previous_operations" => [
                        "00207134365ce71dca6bd7c31d04bfb3244b29897ab538906216fc8ff3d6189410ad",
                    ],
                    "fields" => {
                        "name" => "Telephones",
                    },
                })
                .unwrap()
            )
        )
        .is_ok());

        assert!(validate_cbor(
            &SCHEMA_V1_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "delete",
                    "schema" => "schema_definition_v1",
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
    fn invalid_schema_definition_v1() {
        assert!(validate_cbor(
            &SCHEMA_V1_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "create",
                    "schema" => "schema_definition_v1",
                    "version" => 1,
                    "fields" => {
                        "name" => "Locations",
                        "description" => "Holds information about places",
                        // "fields" missing
                    },
                })
                .unwrap()
            )
        )
        .is_err());

        assert!(validate_cbor(
            &SCHEMA_V1_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "create",
                    "schema" => "schema_definition_v1",
                    "version" => 1,
                    "fields" => {
                        "name" => "Locations",
                        "description" => "Holds information about places",
                        // "field_type" is an unknown field
                        "field_type" => "What am I doing here?",
                        "fields" => [
                            [
                                "00206de69fe88aa24e0929bad2fc9808a0ce2aad8e6d8fb914f4a9178995a56b3435"
                            ]
                        ],
                    },
                })
                .unwrap()
            )
        ).is_err());

        assert!(validate_cbor(
            &SCHEMA_V1_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "update",
                    "schema" => "schema_definition_v1",
                    "version" => 1,
                    "previous_operations" => [
                        "00207134365ce71dca6bd7c31d04bfb3244b29897ab538906216fc8ff3d6189410ad",
                    ],
                    "fields" => {
                        "name" => 12,
                    },
                })
                .unwrap()
            )
        )
        .is_err());
    }

    #[test]
    fn valid_schema_field_definition_v1() {
        assert!(validate_cbor(
            &SCHEMA_FIELD_V1_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "create",
                    "schema" => "schema_field_definition_v1",
                    "version" => 1,
                    "fields" => {
                        "name" => "Size",
                        "description" => "In centimeters",
                        "field_type" => "float",
                    },
                })
                .unwrap()
            )
        )
        .is_ok());

        assert!(validate_cbor(
            &SCHEMA_FIELD_V1_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "update",
                    "schema" => "schema_field_definition_v1",
                    "version" => 1,
                    "previous_operations" => [
                        "00208a5cbba0facc96f22fe3c283e05706c74801282bb7ba315fb5c77caa44689846",
                        "0020e967334f97ac477bf1f53568e475376ae28687e272de3f3d0672ec6f2aa9be53",
                    ],
                    "fields" => {
                        "field_type" => "relation",
                        "field_type" => "relation_list",
                        "field_type" => "pinned_relation",
                        "field_type" => "pinned_relation_list",
                    },
                })
                .unwrap()
            )
        )
        .is_ok());

        assert!(validate_cbor(
            &SCHEMA_FIELD_V1_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "delete",
                    "schema" => "schema_field_definition_v1",
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

    #[test]
    fn invalid_schema_field_definition_v1() {
        assert!(validate_cbor(
            &SCHEMA_FIELD_V1_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "create",
                    "schema" => "schema_field_definition_v1",
                    "version" => 1,
                    "fields" => {
                        "name" => "Size",
                        // "description" field missing
                        "field_type" => "float"
                    },
                })
                .unwrap()
            )
        )
        .is_err());

        assert!(validate_cbor(
            &SCHEMA_FIELD_V1_FORMAT,
            &to_cbor(
                cbor!({
                    "action" => "update",
                    "schema" => "schema_field_definition_v1",
                    "version" => 1,
                    "previous_operations" => [
                        "00209caa5f232debd2835e35a673d5eb148ea803a272c6ca004cd86cbe4a834718d5",
                    ],
                    "fields" => {
                        "field_type" => "beaver_nest",
                    },
                })
                .unwrap()
            )
        )
        .is_err());
    }
}
