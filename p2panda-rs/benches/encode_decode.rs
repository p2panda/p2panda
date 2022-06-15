// SPDX-License-Identifier: AGPL-3.0-or-later

//! Benchmark the performance of encoding and decoding entries and operations.
//!
//! An [`Entry`] and accompanying [`Operation`] are encoded and decoded for varying payload sizes
//! and throughput is measured.

use std::collections::BTreeMap;
use std::convert::TryFrom;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use p2panda_rs::schema::{FieldType, Schema};
use p2panda_rs::{
    entry::{decode_entry, EntrySigned},
    identity::KeyPair,
};
use p2panda_rs::{
    entry::{sign_and_encode, Entry, LogId, SeqNum},
    operation::{Operation, OperationEncoded, OperationFields, OperationValue},
    schema::SchemaId,
};
use rand::{distributions::Alphanumeric, thread_rng, Rng};

/// Encode an [`Entry`] and [`Operation`] given some string payload
fn run_encode(
    payload: &str,
    key_pair: &KeyPair,
    schema: &Schema,
) -> (EntrySigned, OperationEncoded) {
    let mut fields = OperationFields::new();
    fields
        .add("payload", OperationValue::Text(payload.to_owned()))
        .unwrap();

    let operation = Operation::new_create(schema.id().to_owned(), fields).unwrap();

    let entry = Entry::new(
        &LogId::default(),
        Some(&operation),
        None,
        None,
        &SeqNum::new(1).unwrap(),
    )
    .unwrap();

    let entry_encoded = sign_and_encode(&entry, key_pair).unwrap();
    let operation_encoded = OperationEncoded::try_from(&operation).unwrap();
    (entry_encoded, operation_encoded)
}

/// Decode an [`Entry`] and [`Operation`] from their encoded forms.
fn run_decode(entry_encoded: &EntrySigned, operation_encoded: &OperationEncoded, schema: &Schema) {
    // Don't decode entry here because we do it in the following line.
    decode_entry(entry_encoded, None, None).unwrap();
    operation_encoded.decode(schema).unwrap();
}

/// Construct a random string given a size.
fn random_string(size: usize) -> String {
    let mut rng = thread_rng();
    (0..size)
        .map(|_| rng.sample(Alphanumeric) as char)
        .collect()
}

fn get_benchmark_id(function_name: &str, size: &usize) -> BenchmarkId {
    static KB: usize = 1024;
    let benchmark_parameter = match size > &KB {
        false => format!("{} B", size),
        true => format!("{} KiB", size / KB),
    };
    BenchmarkId::new(function_name, benchmark_parameter)
}

fn criterion_benchmark(c: &mut Criterion) {
    static KB: usize = 1024;
    let key_pair = KeyPair::new();

    // This schema doesn't correspond to an actually published schema.
    let mut fields = BTreeMap::new();
    fields.insert("payload".to_string(), FieldType::String);
    let schema = Schema::new_definition(
        SchemaId::new(
            "benchmark_0020d3ce4e85222017ffcb4e5ee032716e2e391478379a29e25bc35d74dd614e4132",
        )
        .unwrap(),
        "".to_string(),
        fields,
    );

    // Test encoding performance for a range of payload sizes
    let mut encode_decode = c.benchmark_group("entry and operation");
    for size in [16, KB, 16 * KB, 64 * KB, 256 * KB, 1024 * KB].iter() {
        let payload = random_string(*size);

        encode_decode.throughput(Throughput::Bytes(*size as u64));
        encode_decode.bench_with_input(get_benchmark_id("encode", size), size, |b, &_size| {
            b.iter(|| run_encode(&payload, &key_pair, &schema))
        });
    }

    // Test decoding performance for a range of payload sizes
    for size in [16, KB, 16 * KB, 64 * KB, 256 * KB, 1024 * KB].iter() {
        let payload = random_string(*size);
        let (entry_encoded, operation_encoded) = run_encode(&payload, &key_pair, &schema);

        encode_decode.throughput(Throughput::Bytes(*size as u64));
        encode_decode.bench_with_input(get_benchmark_id("decode", size), size, |b, &_size| {
            b.iter(|| run_decode(&entry_encoded, &operation_encoded, &schema))
        });
    }
    encode_decode.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
