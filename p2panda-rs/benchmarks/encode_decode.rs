// SPDX-License-Identifier: AGPL-3.0-or-later

//! Benchmark the performance of encoding and decoding entries and operations while also performing
//! a full validation against a schema.
//!
//! An [`Entry`] and accompanying [`Operation`] are encoded and decoded for varying payload sizes
//! and throughput is measured.
use std::convert::TryInto;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use p2panda_rs::document::DocumentViewId;
use p2panda_rs::entry::decode::decode_entry;
use p2panda_rs::entry::encode::encode_entry;
use p2panda_rs::entry::{EncodedEntry, Entry, EntryBuilder};
use p2panda_rs::hash::Hash;
use p2panda_rs::identity::KeyPair;
use p2panda_rs::operation::decode::decode_operation;
use p2panda_rs::operation::encode::encode_operation;
use p2panda_rs::operation::validate::validate_operation_with_entry;
use p2panda_rs::operation::{EncodedOperation, OperationBuilder};
use p2panda_rs::schema::{FieldType, Schema, SchemaId};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

/// Encode an [`Entry`] and [`Operation`] given some string payload.
fn run_encode(
    payload: &str,
    key_pair: &KeyPair,
    schema_id: &SchemaId,
) -> (EncodedEntry, EncodedOperation) {
    let operation = OperationBuilder::new(schema_id)
        .fields(&[("payload", payload.into())])
        .build()
        .unwrap();
    let encoded_operation = encode_operation(&operation).unwrap();

    let entry = EntryBuilder::new()
        .log_id(&0.into())
        .seq_num(&1.try_into().unwrap())
        .sign(&encoded_operation, key_pair)
        .unwrap();
    let encoded_entry = encode_entry(&entry).unwrap();

    (encoded_entry, encoded_operation)
}

/// Decode an [`Entry`] and [`Operation`] from byte encodings.
fn run_decode(encoded_entry: &EncodedEntry, encoded_operation: &EncodedOperation, schema: &Schema) {
    let entry = decode_entry(encoded_entry).unwrap();

    let plain_operation = decode_operation(encoded_operation).unwrap();
    validate_operation_with_entry(
        &entry,
        encoded_entry,
        None::<(&Entry, &Hash)>,
        None::<(&Entry, &Hash)>,
        &plain_operation,
        encoded_operation,
        schema,
    )
    .unwrap();
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

    let schema_view_id = DocumentViewId::new(&[
        "00201413ae916e6745ab715c1f5ab49c47d6773c3c0febd970ecf1039beed203b472"
            .parse()
            .unwrap(),
    ]);
    let schema_id = SchemaId::Application("benchmark".to_string(), schema_view_id);
    let schema = Schema::new(
        &schema_id,
        "Payload for measuring performance of encoding, decoding and validation",
        vec![("payload", FieldType::String)],
    )
    .unwrap();

    // Test encoding performance for a range of payload sizes
    let mut encode_decode = c.benchmark_group("entry and operation");
    for size in [16, KB, 16 * KB, 64 * KB, 256 * KB, 1024 * KB].iter() {
        let payload = random_string(*size);

        encode_decode.throughput(Throughput::Bytes(*size as u64));
        encode_decode.bench_with_input(get_benchmark_id("encode", size), size, |b, &_size| {
            b.iter(|| run_encode(&payload, &key_pair, &schema_id))
        });
    }

    // Test decoding performance for a range of payload sizes
    for size in [16, KB, 16 * KB, 64 * KB, 256 * KB, 1024 * KB].iter() {
        let payload = random_string(*size);
        let (entry_encoded, operation_encoded) = run_encode(&payload, &key_pair, &schema_id);

        encode_decode.throughput(Throughput::Bytes(*size as u64));
        encode_decode.bench_with_input(get_benchmark_id("decode", size), size, |b, &_size| {
            b.iter(|| run_decode(&entry_encoded, &operation_encoded, &schema))
        });
    }
    encode_decode.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
