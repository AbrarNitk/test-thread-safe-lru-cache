use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use test_thread_safe_lru_cache::sharded::cache::Cache;

fn bench_sequential_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("Sequential Operations");

    // Setup a standard cache with 4 shards and 10,000 total capacity
    let cache = Cache::lru(10_000, 4);

    group.bench_function("Sequential Push", |b| {
        let mut key = 0;
        b.iter(|| {
            key += 1;
            cache.push(black_box(key), black_box(key));
        })
    });

    group.bench_function("Sequential Get (Hit/Miss)", |b| {
        // Pre-populate some values
        for i in 0..1000 {
            cache.push(i, i);
        }
        let mut key = 0;
        b.iter(|| {
            key = (key + 1) % 2000; // 50% hits, 50% misses
            black_box(cache.get(&key));
        })
    });

    group.finish();
}

fn bench_concurrent_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("Concurrent Contention");

    // We vary the shard counts to prove that scaling shards scales throughput
    for shard_count in [1, 2, 4, 8, 16] {
        group.bench_with_input(
            BenchmarkId::new("Shard Count", shard_count),
            &shard_count,
            |b, &shards| {
                // Using iter_custom handles thread pooling elegantly:
                // We let Criterion decide how many iterations (`iters`) it needs,
                // then divide that workload across our thread workers.
                b.iter_custom(|iters| {
                    let cache = Arc::new(Cache::lru(10_000, shards));
                    let num_threads = 4;
                    let ops_per_thread = (iters as usize / num_threads).max(1);

                    // Pre-fill cache to ensure lock contention hits updates + eviction legs
                    for i in 0..2000 {
                        cache.push(i, i);
                    }

                    let start = Instant::now();
                    std::thread::scope(|s| {
                        for t in 0..num_threads {
                            let cache = Arc::clone(&cache);
                            s.spawn(move || {
                                for i in 0..ops_per_thread {
                                    // Generate worker-specific keys to simulate high concurrency
                                    let key = (t * 1_000_000) + (i % 5000);

                                    if i % 4 == 0 {
                                        // 25% Writes (Evictions/Insertions)
                                        cache.push(key, key);
                                    } else {
                                        // 75% Reads
                                        black_box(cache.get(&key));
                                    }
                                }
                            });
                        }
                    });

                    // Return total elapsed time for all operations combined
                    start.elapsed()
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_sequential_operations,
    bench_concurrent_contention
);
criterion_main!(benches);
