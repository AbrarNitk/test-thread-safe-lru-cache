//! latency.rs — HDR-histogram per-operation latency benchmark
//!
//! Measures tail latency for all three cache implementations:
//!   • basic::ThreadSafeLru   — single global Mutex
//!   • sharded::Cache::lru    — per-shard RwLock + deferred recency
//!   • sharded::Cache::fifo   — per-shard RwLock, FIFO eviction
//!
//! Outputs:
//!   • Console: pretty-printed table of p50 / p95 / p99 / p999 in nanoseconds
//!   • File:    `latency_results.csv` (suitable for plotting or diffing in CI)
//!
//! Run:
//!   cargo bench --bench latency

use criterion::{Criterion, criterion_group, criterion_main};
use hdrhistogram::Histogram;
use std::{
    fs::File,
    io::{BufWriter, Write},
    sync::Arc,
    time::Instant,
};
use test_thread_safe_lru_cache::{
    basic::safe::ThreadSafeLru,
    sharded::{
        cache::Cache,
        eviction::{fifo::Fifo, lru::Lru},
    },
};

// ── Config ───────────────────────────────────────────────────────────────────

const CAPACITY: usize = 10_000;
const DEFAULT_SHARDS: usize = 4;
const OPS_PER_BENCH: usize = 500_000; // enough for stable percentiles
const WRITE_PCT: usize = 25; // 25% writes, 75% reads
const KEY_SPACE: u32 = 5_000;

// ── HDR histogram wrapper ─────────────────────────────────────────────────────

/// Build a histogram capable of tracking latencies up to 10 seconds with 3
/// significant figures of precision.
fn make_hist() -> Histogram<u64> {
    Histogram::<u64>::new_with_bounds(1, 10_000_000_000, 3).expect("valid HDR histogram bounds")
}

/// Print a latency table to stdout.
fn print_table(label: &str, hist: &Histogram<u64>) {
    println!(
        "  {:<30}  p50={:>8}ns  p95={:>8}ns  p99={:>8}ns  p999={:>9}ns  max={:>9}ns",
        label,
        hist.value_at_quantile(0.50),
        hist.value_at_quantile(0.95),
        hist.value_at_quantile(0.99),
        hist.value_at_quantile(0.999),
        hist.max(),
    );
}

/// Append one row to the CSV writer.
fn write_csv_row(w: &mut impl Write, label: &str, threads: usize, hist: &Histogram<u64>) {
    writeln!(
        w,
        "{},{},{},{},{},{},{}",
        label,
        threads,
        hist.value_at_quantile(0.50),
        hist.value_at_quantile(0.95),
        hist.value_at_quantile(0.99),
        hist.value_at_quantile(0.999),
        hist.max(),
    )
    .expect("csv write failed");
}

// ── Core measurement function ─────────────────────────────────────────────────

/// Run `OPS_PER_BENCH` operations across `num_threads` threads, recording
/// per-operation latency into a returned histogram.
///
/// The histogram is built from thread-local histograms that are merged at the
/// end — this avoids synchronisation overhead on the hot path.
fn measure<F, G>(num_threads: usize, push_fn: Arc<F>, get_fn: Arc<G>) -> Histogram<u64>
where
    F: Fn(u32, u32) + Send + Sync + 'static,
    G: Fn(u32) -> bool + Send + Sync + 'static,
{
    let ops_per_thread = (OPS_PER_BENCH / num_threads).max(1);

    // Collect thread-local histograms and merge at the end.
    let local_hists: Vec<Histogram<u64>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..num_threads)
            .map(|t| {
                let push_fn = Arc::clone(&push_fn);
                let get_fn = Arc::clone(&get_fn);
                s.spawn(move || {
                    let mut hist = make_hist();
                    for i in 0..ops_per_thread {
                        let key =
                            ((t as u32).wrapping_mul(1_000_003).wrapping_add(i as u32)) % KEY_SPACE;
                        let is_write = i % 100 < WRITE_PCT;

                        let t0 = Instant::now();
                        if is_write {
                            push_fn(key, key);
                        } else {
                            let _ = std::hint::black_box(get_fn(key));
                        }
                        let elapsed = t0.elapsed().as_nanos() as u64;
                        // HDR histogram saturates silently on overflow, which is fine.
                        let _ = hist.record(elapsed.max(1));
                    }
                    hist
                })
            })
            .collect();

        handles
            .into_iter()
            .map(|h| h.join().expect("thread panicked"))
            .collect()
    });

    let mut merged = make_hist();
    for local in local_hists {
        merged.add(&local).expect("histogram merge failed");
    }
    merged
}

// ── Criterion bench wrappers ──────────────────────────────────────────────────
//
// Criterion is used here primarily to:
//   1. Hook into the standard `cargo bench` workflow.
//   2. Give us warm-up iterations so the allocator / CPU caches are hot.
//
// The actual latency data comes from our HDR histograms recorded inside the
// bencher closure, and is printed/written after all runs.

fn bench_latency_all(c: &mut Criterion) {
    // Collect all results into this Vec so we can write a single CSV at the end.
    let mut results: Vec<(String, usize, Histogram<u64>)> = Vec::new();

    // ── thread counts to probe ───────────────────────────────────────────────
    let thread_counts = [1usize, 2, 4, 8];

    println!("\n{}", "=".repeat(100));
    println!("  LATENCY BENCHMARK  (HDR Histogram — nanoseconds per operation)");
    println!("{}\n", "=".repeat(100));

    // ── Basic global-mutex LRU ───────────────────────────────────────────────
    {
        let mut group = c.benchmark_group("latency/basic_global_mutex");
        for &threads in &thread_counts {
            let cache = Arc::new(ThreadSafeLru::<u32, u32>::new(CAPACITY));
            // pre-fill
            for i in 0..KEY_SPACE.min(CAPACITY as u32) {
                cache.push(i, i);
            }

            let cache_push = Arc::clone(&cache);
            let cache_get = Arc::clone(&cache);
            let push_fn = Arc::new(move |k: u32, v: u32| cache_push.push(k, v));
            let get_fn = Arc::new(move |k: u32| cache_get.get(&k).is_some());

            group.bench_function(format!("{}t", threads), |b| {
                b.iter(|| {
                    // Just one "warm" op so Criterion sees timing; real data comes
                    // from the separate HDR measurement below.
                    let _ = cache.get(&0u32);
                });
            });

            let hist = measure(threads, Arc::clone(&push_fn), Arc::clone(&get_fn));
            let label = format!("basic_global_mutex_{}t", threads);
            print_table(&label, &hist);
            results.push((label, threads, hist));
        }
        group.finish();
    }

    println!();

    // ── Sharded LRU ──────────────────────────────────────────────────────────
    {
        let mut group = c.benchmark_group("latency/sharded_lru");
        for &threads in &thread_counts {
            let cache = Arc::new(Cache::<u32, u32, Lru<u32, u32>>::lru(
                CAPACITY,
                DEFAULT_SHARDS,
            ));
            for i in 0..KEY_SPACE.min(CAPACITY as u32) {
                cache.push(i, i);
            }

            let cache_push = Arc::clone(&cache);
            let cache_get = Arc::clone(&cache);
            let push_fn = Arc::new(move |k: u32, v: u32| cache_push.push(k, v));
            let get_fn = Arc::new(move |k: u32| cache_get.get(&k).is_some());

            group.bench_function(format!("{}t", threads), |b| {
                b.iter(|| {
                    let _ = cache.get(&0u32);
                });
            });

            let hist = measure(threads, Arc::clone(&push_fn), Arc::clone(&get_fn));
            let label = format!("sharded_lru_4shards_{}t", threads);
            print_table(&label, &hist);
            results.push((label, threads, hist));
        }
        group.finish();
    }

    println!();

    // ── Sharded FIFO ─────────────────────────────────────────────────────────
    {
        let mut group = c.benchmark_group("latency/sharded_fifo");
        for &threads in &thread_counts {
            let cache = Arc::new(Cache::<u32, u32, Fifo<u32, u32>>::fifo(
                CAPACITY,
                DEFAULT_SHARDS,
            ));
            for i in 0..KEY_SPACE.min(CAPACITY as u32) {
                cache.push(i, i);
            }

            let cache_push = Arc::clone(&cache);
            let cache_get = Arc::clone(&cache);
            let push_fn = Arc::new(move |k: u32, v: u32| cache_push.push(k, v));
            let get_fn = Arc::new(move |k: u32| cache_get.get(&k).is_some());

            group.bench_function(format!("{}t", threads), |b| {
                b.iter(|| {
                    let _ = cache.get(&0u32);
                });
            });

            let hist = measure(threads, Arc::clone(&push_fn), Arc::clone(&get_fn));
            let label = format!("sharded_fifo_4shards_{}t", threads);
            print_table(&label, &hist);
            results.push((label, threads, hist));
        }
        group.finish();
    }

    // ── Write CSV ─────────────────────────────────────────────────────────────
    let csv_path = "latency_results.csv";
    let f = File::create(csv_path).expect("cannot create latency_results.csv");
    let mut w = BufWriter::new(f);
    writeln!(w, "impl,threads,p50_ns,p95_ns,p99_ns,p999_ns,max_ns")
        .expect("csv header write failed");
    for (label, threads, hist) in &results {
        write_csv_row(&mut w, label, *threads, hist);
    }
    drop(w);
    println!("\n  ✓  latency results written → {}\n", csv_path);
}

// ── Entry point ───────────────────────────────────────────────────────────────

criterion_group!(benches, bench_latency_all);
criterion_main!(benches);
