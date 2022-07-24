// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::operation::{verify_schema_and_convert, Operation, RawOperation, RawOperationError};
use crate::schema::Schema;

pub fn decode_operation(bytes: &[u8], schema: &Schema) -> Result<Operation, RawOperationError> {
    let raw_operation: RawOperation = ciborium::de::from_reader(bytes)
        .map_err(|err| RawOperationError::InvalidCBOREncoding(err.to_string()))?;

    let operation = verify_schema_and_convert(&raw_operation, schema)?;
    Ok(operation)
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use ciborium::value::{Error, Value};
    use rstest::rstest;

    use crate::schema::{FieldType, Schema, SchemaId};
    use crate::test_utils::constants::{HASH, SCHEMA_ID};
    use crate::test_utils::fixtures::schema_id;

    use super::decode_operation;

    fn encode_cbor(value: Value) -> Vec<u8> {
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&value, &mut cbor_bytes).unwrap();
        cbor_bytes
    }

    #[rstest]
    #[case(
        vec![
            ("country", FieldType::Relation(schema_id.clone())),
            ("national_dish", FieldType::Text),
            ("vegan_friendly", FieldType::Boolean),
            ("yummyness", FieldType::Integer),
            ("yumsimumsiness", FieldType::Float),
        ],
        cbor!([
            1, 0, SCHEMA_ID,
            {
                "country" => HASH,
                "national_dish" => "Pumpkin",
                "vegan_friendly" => true,
                "yummyness" => 8,
                "yumsimumsiness" => 7.2,
            },
        ]),
    )]
    fn valid_operations(
        #[from(schema_id)] schema_id: SchemaId,
        #[case] schema_fields: Vec<(&str, FieldType)>,
        #[case] raw_operation: Result<Value, Error>,
    ) {
        let schema = Schema::new(&schema_id, "Some schema description", schema_fields)
            .expect("Could not create schema");

        let bytes = encode_cbor(raw_operation.expect("Invalid CBOR value"));
        println!("{}", hex::encode(&bytes));
        decode_operation(&bytes, &schema).unwrap();
    }
}
