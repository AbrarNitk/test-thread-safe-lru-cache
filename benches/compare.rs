//! compare.rs — Cross-implementation Criterion benchmark
//!
//! Compares three cache implementations under identical workloads:
//!   • basic::ThreadSafeLru   — single global Mutex
//!   • sharded::Cache::lru    — per-shard RwLock + deferred recency
//!   • sharded::Cache::fifo   — per-shard RwLock, FIFO eviction
//!
//! Workloads:
//!   • read_heavy  — 80% get / 20% push
//!   • write_heavy — 20% get / 80% push
//!   • balanced    — 50% get / 50% push
//!   • hot_key     — 25% push / 75% get over a tiny 100-key space
//!
//! Thread counts: 1, 2, 4, 8
//!
//! Run:
//!   cargo bench --bench compare
//!
//! Save a named baseline for regression tracking:
//!   cargo bench --bench compare -- --save-baseline main
//!
//! Compare against a saved baseline:
//!   cargo bench --bench compare -- --load-baseline main --baseline main

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::{sync::Arc, time::Instant};
use test_thread_safe_lru_cache::{basic::safe::ThreadSafeLru, sharded::cache::Cache};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Total logical capacity of every cache under test.
const CAPACITY: usize = 10_000;
/// Shard count used for the sharded impls in all non-shard-scaling groups.
const DEFAULT_SHARDS: usize = 4;

// ── Cache abstraction ─────────────────────────────────────────────────────────

/// Minimal trait so we can drive all three impls with a single workload closure.
trait BenchCache: Send + Sync + 'static {
    fn bench_push(&self, key: u32, value: u32);
    fn bench_get(&self, key: u32) -> bool;
}

// -- basic ThreadSafeLru -------------------------------------------------------
struct BasicLru(ThreadSafeLru<u32, u32>);

impl BenchCache for BasicLru {
    #[inline]
    fn bench_push(&self, key: u32, value: u32) {
        self.0.push(key, value);
    }
    #[inline]
    fn bench_get(&self, key: u32) -> bool {
        self.0.get(&key).is_some()
    }
}

// -- sharded Lru --------------------------------------------------------------
use test_thread_safe_lru_cache::sharded::eviction::lru::Lru as ShardedLruEviction;

struct ShardedLru(Cache<u32, u32, ShardedLruEviction<u32, u32>>);

impl BenchCache for ShardedLru {
    #[inline]
    fn bench_push(&self, key: u32, value: u32) {
        self.0.push(key, value);
    }
    #[inline]
    fn bench_get(&self, key: u32) -> bool {
        self.0.get(&key).is_some()
    }
}

// -- sharded Fifo -------------------------------------------------------------
use test_thread_safe_lru_cache::sharded::eviction::fifo::Fifo as ShardedFifoEviction;

struct ShardedFifo(Cache<u32, u32, ShardedFifoEviction<u32, u32>>);

impl BenchCache for ShardedFifo {
    #[inline]
    fn bench_push(&self, key: u32, value: u32) {
        self.0.push(key, value);
    }
    #[inline]
    fn bench_get(&self, key: u32) -> bool {
        self.0.get(&key).is_some()
    }
}

// ── Workload driver ───────────────────────────────────────────────────────────

/// Runs `iters` operations spread across `num_threads` threads using `cache`.
///
/// * `write_pct`  — percentage (0–100) of operations that are pushes.
/// * `key_space`  — key universe size; smaller = higher hit-rate / more contention.
fn run_workload(
    b: &mut criterion::Bencher,
    cache: Arc<dyn BenchCache>,
    num_threads: usize,
    write_pct: u32,
    key_space: u32,
) {
    // Pre-fill so reads have something to hit.
    for i in 0..key_space.min(CAPACITY as u32) {
        cache.bench_push(i, i);
    }

    b.iter_custom(|iters| {
        let ops_per_thread = ((iters as usize) / num_threads).max(1);
        let start = Instant::now();

        std::thread::scope(|s| {
            for t in 0..num_threads {
                let cache = Arc::clone(&cache);
                s.spawn(move || {
                    for i in 0..ops_per_thread {
                        // Distribute keys across workers to avoid false sharing.
                        let key =
                            ((t as u32).wrapping_mul(1_000_003).wrapping_add(i as u32)) % key_space;
                        let is_write = (i as u32 % 100) < write_pct;
                        if is_write {
                            cache.bench_push(key, key);
                        } else {
                            let _ = std::hint::black_box(cache.bench_get(key));
                        }
                    }
                });
            }
        });

        start.elapsed()
    });
}

// ── Helper: build all three caches at the same capacity ──────────────────────

fn make_basic(capacity: usize) -> Arc<dyn BenchCache> {
    Arc::new(BasicLru(ThreadSafeLru::new(capacity)))
}

fn make_sharded_lru(capacity: usize, shards: usize) -> Arc<dyn BenchCache> {
    Arc::new(ShardedLru(Cache::lru(capacity, shards)))
}

fn make_sharded_fifo(capacity: usize, shards: usize) -> Arc<dyn BenchCache> {
    Arc::new(ShardedFifo(Cache::fifo(capacity, shards)))
}

// ── Benchmark groups ──────────────────────────────────────────────────────────

/// Compares all three impls under four workload types, single-threaded.
fn bench_workload_comparison(c: &mut Criterion) {
    struct Workload {
        name: &'static str,
        write_pct: u32,
        key_space: u32,
    }

    let workloads = [
        Workload {
            name: "read_heavy_80_20",
            write_pct: 20,
            key_space: 5_000,
        },
        Workload {
            name: "write_heavy_20_80",
            write_pct: 80,
            key_space: 5_000,
        },
        Workload {
            name: "balanced_50_50",
            write_pct: 50,
            key_space: 5_000,
        },
        Workload {
            name: "hot_key_25_75",
            write_pct: 25,
            key_space: 100,
        },
    ];

    for wl in &workloads {
        let mut group = c.benchmark_group(format!("workload/{}", wl.name));
        group.throughput(Throughput::Elements(1));

        let (wp, ks) = (wl.write_pct, wl.key_space);

        group.bench_function("basic_global_mutex", |b| {
            run_workload(b, make_basic(CAPACITY), 1, wp, ks);
        });
        group.bench_function("sharded_lru_4", |b| {
            run_workload(b, make_sharded_lru(CAPACITY, DEFAULT_SHARDS), 1, wp, ks);
        });
        group.bench_function("sharded_fifo_4", |b| {
            run_workload(b, make_sharded_fifo(CAPACITY, DEFAULT_SHARDS), 1, wp, ks);
        });

        group.finish();
    }
}

/// Scales thread count (1 → 2 → 4 → 8) and compares all three impls.
/// Workload: balanced 50/50, 5 000-key space.
fn bench_thread_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("thread_scaling/balanced_50_50");
    group.throughput(Throughput::Elements(1));

    for &threads in &[1usize, 2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("basic_global_mutex", threads),
            &threads,
            |b, &t| run_workload(b, make_basic(CAPACITY), t, 50, 5_000),
        );
        group.bench_with_input(
            BenchmarkId::new("sharded_lru_4", threads),
            &threads,
            |b, &t| run_workload(b, make_sharded_lru(CAPACITY, DEFAULT_SHARDS), t, 50, 5_000),
        );
        group.bench_with_input(
            BenchmarkId::new("sharded_fifo_4", threads),
            &threads,
            |b, &t| run_workload(b, make_sharded_fifo(CAPACITY, DEFAULT_SHARDS), t, 50, 5_000),
        );
    }
    group.finish();
}

/// Scales shard count for the LRU impl at fixed 8 threads, balanced workload.
/// The basic impl is included as a 1-shard baseline for comparison.
fn bench_shard_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("shard_scaling/lru_8threads");
    group.throughput(Throughput::Elements(1));

    // baseline: basic (effectively 1 shard, global lock)
    group.bench_function("basic_global_mutex_1shard", |b| {
        run_workload(b, make_basic(CAPACITY), 8, 50, 5_000);
    });

    for &shards in &[1usize, 2, 4, 8, 16] {
        group.bench_with_input(BenchmarkId::new("sharded_lru", shards), &shards, |b, &s| {
            run_workload(b, make_sharded_lru(CAPACITY, s), 8, 50, 5_000)
        });
    }
    group.finish();
}

/// Head-to-head: sharded LRU vs sharded FIFO across multiple thread counts.
fn bench_lru_vs_fifo(c: &mut Criterion) {
    let mut group = c.benchmark_group("lru_vs_fifo/sharded_4shards");
    group.throughput(Throughput::Elements(1));

    for &threads in &[1usize, 2, 4, 8] {
        group.bench_with_input(BenchmarkId::new("lru", threads), &threads, |b, &t| {
            run_workload(b, make_sharded_lru(CAPACITY, DEFAULT_SHARDS), t, 50, 5_000)
        });
        group.bench_with_input(BenchmarkId::new("fifo", threads), &threads, |b, &t| {
            run_workload(b, make_sharded_fifo(CAPACITY, DEFAULT_SHARDS), t, 50, 5_000)
        });
    }
    group.finish();
}

// ── Entry point ───────────────────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_workload_comparison,
    bench_thread_scaling,
    bench_shard_scaling,
    bench_lru_vs_fifo,
);
criterion_main!(benches);
