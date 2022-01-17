// Benchmarking tests adapted from [https://github.com/declanvk/incremental-topo/tree/main/benches](https://github.com/declanvk/incremental-topo/tree/main/benches).

#[macro_use]
extern crate criterion;
extern crate rand;

use criterion::{BenchmarkId, Criterion};
use incremental_topo::IncrementalTopo;

use p2panda_rs::materialiser::Graph;

const DEFAULT_DENSITY: f32 = 0.1;
const DEFAULT_SIZE: u64 = 100;

fn generate_random_p2panda_dag(size: u64, density: f32) -> Graph<u64> {
    use rand::distributions::{Bernoulli, Distribution};
    assert!(0.0 < density && density <= 1.0);
    let mut rng = rand::thread_rng();
    let dist = Bernoulli::new(density.into());
    let mut topo = Graph::new();

    for node in 0..size {
        topo.add_node(&node.to_string(), node);
    }

    for i in 0..size {
        for j in 1..size {
            if i != j && dist.unwrap().sample(&mut rng) {
                // Ignore failures
                let _ = topo.add_link(&i.to_string(), &j.to_string());
            }
        }
    }

    topo
}

fn generate_random_incremental_topo_dag(size: u64, density: f32) -> IncrementalTopo<u64> {
    use rand::distributions::{Bernoulli, Distribution};
    assert!(0.0 < density && density <= 1.0);
    let mut rng = rand::thread_rng();
    let dist = Bernoulli::new(density.into());
    let mut topo = IncrementalTopo::new();

    for node in 0..size {
        topo.add_node(node);
    }

    for i in 0..size {
        for j in 1..size {
            if i != j && dist.unwrap().sample(&mut rng) {
                // Ignore failures
                let _ = topo.add_dependency(&i, &j);
            }
        }
    }

    topo
}
fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("build_graph_different_density");
    for density in [0.01, 0.03, 0.05, 0.07, 0.09] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("p2panda: {}", density)),
            &density,
            |b, density| {
                b.iter(|| {
                    let _p2panda_dag = generate_random_p2panda_dag(DEFAULT_SIZE, *density);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("incremental-topo: {}", density)),
            &density,
            |b, density| {
                b.iter(|| {
                    let _incremental_topo_dag =
                        generate_random_incremental_topo_dag(DEFAULT_SIZE, *density);
                });
            },
        );
    }
    group.finish();

    let mut group = c.benchmark_group("build_graph_different_size");
    for size in [10, 100, 1000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("p2panda: {}", size)),
            &size,
            |b, size| {
                b.iter(|| {
                    let _p2panda_dag = generate_random_p2panda_dag(*size, DEFAULT_DENSITY);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("incremental-topo: {}", size)),
            &size,
            |b, size| {
                b.iter(|| {
                    let _incremental_topo_dag =
                        generate_random_incremental_topo_dag(*size, DEFAULT_DENSITY);
                });
            },
        );
    }
    group.finish();

    let mut group = c.benchmark_group("walk_random_graph_different_density");
    for density in [0.01, 0.03, 0.05, 0.07, 0.09] {
        let dag = generate_random_p2panda_dag(DEFAULT_SIZE, density);
        let incremental_topo_dag = generate_random_incremental_topo_dag(DEFAULT_SIZE, density);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("p2panda: {}", density)),
            &dag,
            |b, dag| {
                b.iter(|| {
                    let mut dag = dag.clone();
                    let _ = dag.walk_from(&0.to_string());
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("incremental-topo: {}", density)),
            &incremental_topo_dag,
            |b, dag| {
                b.iter(|| {
                    let dag = dag.clone();
                    let _ = dag.descendants(&0);
                });
            },
        );
    }
    group.finish();

    let mut group = c.benchmark_group("walk_random_graph_different_sizes");
    for size in [10, 100, 1000] {
        let dag = generate_random_p2panda_dag(size, DEFAULT_DENSITY);
        let incremental_topo_dag = generate_random_incremental_topo_dag(size, DEFAULT_DENSITY);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("p2panda: {}", size)),
            &dag,
            |b, dag| {
                b.iter(|| {
                    let mut dag = dag.clone();
                    let _ = dag.walk_from(&0.to_string());
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("incremental-topo: {}", size)),
            &incremental_topo_dag,
            |b, dag| {
                b.iter(|| {
                    let dag = dag.clone();
                    let _ = dag.descendants(&0);
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
