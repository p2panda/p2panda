// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{convert::{TryFrom}};

use criterion::{Criterion, criterion_group, criterion_main, Throughput, BenchmarkId};
use p2panda_rs::{identity::KeyPair, entry::{decode_entry, EntrySigned}};
use p2panda_rs::{operation::{OperationFields, OperationValue, Operation, OperationEncoded}, schema::SchemaId, entry::{Entry, LogId, SeqNum, sign_and_encode}};
use rand::{thread_rng, distributions::Alphanumeric, Rng};


/// Encode an [`Entry`] and [`Operation`] given some string payload
fn run_encode(payload: &String, key_pair: &KeyPair) -> (EntrySigned, OperationEncoded) {
    let mut fields = OperationFields::new();
    fields
        .add("payload", OperationValue::Text(payload.to_owned()))
        .unwrap();

    let hash = SchemaId::new("0020d3ce4e85222017ffcb4e5ee032716e2e391478379a29e25bc35d74dd614e4132").unwrap();
    let operation = Operation::new_create(hash, fields).unwrap();

    let entry = Entry::new(
       &LogId::default(),
       Some(&operation),
       None,
       None,
       &SeqNum::new(1).unwrap(),
    ).unwrap();

    let entry_encoded = sign_and_encode(&entry, key_pair).unwrap();
    let operation_encoded = OperationEncoded::try_from(&operation).unwrap();
    (entry_encoded, operation_encoded)
}

/// Decode an [`Entry`] and [`Operation`] from their encoded forms
fn run_decode(entry_encoded: &EntrySigned, operation_encoded: &OperationEncoded) {
    decode_entry(entry_encoded, Some(operation_encoded)).unwrap();
    Operation::try_from(operation_encoded).unwrap();
}

/// Construct a random string given a size
fn random_string(size: usize) -> String {
    let mut rng = thread_rng();
    (0..size).map(|_| rng.sample(Alphanumeric) as char).collect()
}

fn criterion_benchmark(c: &mut Criterion) {
    static KB: usize = 1024;
    let key_pair = KeyPair::new();

    // Test encoding performance for a range of payload sizes
    let mut encode_group = c.benchmark_group("encode_payload");
    for size in [16, KB, 16 * KB, 128 * KB, 1024 * KB].iter() {
        let payload = random_string(*size);

        encode_group.throughput(Throughput::Bytes(*size as u64));
        encode_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| run_encode(&payload, &key_pair))
        });
    }
    encode_group.finish();

    // Test decoding performance for a range of payload sizes
    let mut decode_group = c.benchmark_group("decode_payload");
    for size in [16, KB, 16 * KB, 128 * KB, 1024 * KB].iter() {
        let payload = random_string(*size);
        let (entry_encoded, operation_encoded) = run_encode(&payload, &key_pair);

        decode_group.throughput(Throughput::Bytes(*size as u64));
        decode_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| run_decode(&entry_encoded, &operation_encoded))
        });
    }
    decode_group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
