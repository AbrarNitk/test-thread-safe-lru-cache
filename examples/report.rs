//! examples/report.rs — Standalone analytics report generator
//!
//! Runs all three cache implementations for a fixed duration, collects:
//!   • Throughput  (ops/sec)
//!   • Hit rate    (%)
//!   • Latency percentiles  p50 / p95 / p99 / p999  (nanoseconds)
//!
//! Outputs:
//!   • Console  — ASCII summary table
//!   • report.html — Self-contained single-file HTML with inline SVG charts
//!
//! Run:
//!   cargo run --example report --release
//!
//! Dependencies: only what's already in dev-dependencies (hdrhistogram, rand).

use hdrhistogram::Histogram;
use std::{
    fs::File,
    io::{BufWriter, Write},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use test_thread_safe_lru_cache::{
    basic::safe::ThreadSafeLru,
    sharded::{
        cache::Cache,
        eviction::{fifo::Fifo, lru::Lru},
    },
};

// ── Config ────────────────────────────────────────────────────────────────────

const CAPACITY: usize = 10_000;
const DEFAULT_SHARDS: usize = 4;
const BENCH_DURATION: Duration = Duration::from_secs(4);
const THREAD_COUNTS: &[usize] = &[1, 2, 4, 8];
const WRITE_PCT: usize = 25; // 25% writes, 75% reads
const KEY_SPACE: u64 = 5_000;

// ── Trait abstraction ─────────────────────────────────────────────────────────

trait CacheBench: Send + Sync + 'static {
    fn do_push(&self, key: u64, value: u64);
    fn do_get(&self, key: u64) -> bool;
    fn impl_name(&self) -> &'static str;
}

struct BasicLru(ThreadSafeLru<u64, u64>);
impl CacheBench for BasicLru {
    fn do_push(&self, key: u64, value: u64) {
        self.0.push(key, value);
    }
    fn do_get(&self, key: u64) -> bool {
        self.0.get(&key).is_some()
    }
    fn impl_name(&self) -> &'static str {
        "basic::ThreadSafeLru (global Mutex)"
    }
}

struct ShardedLru(Cache<u64, u64, Lru<u64, u64>>);
impl CacheBench for ShardedLru {
    fn do_push(&self, key: u64, value: u64) {
        self.0.push(key, value);
    }
    fn do_get(&self, key: u64) -> bool {
        self.0.get(&key).is_some()
    }
    fn impl_name(&self) -> &'static str {
        "sharded::Cache (LRU, 4 shards)"
    }
}

struct ShardedFifo(Cache<u64, u64, Fifo<u64, u64>>);
impl CacheBench for ShardedFifo {
    fn do_push(&self, key: u64, value: u64) {
        self.0.push(key, value);
    }
    fn do_get(&self, key: u64) -> bool {
        self.0.get(&key).is_some()
    }
    fn impl_name(&self) -> &'static str {
        "sharded::Cache (FIFO, 4 shards)"
    }
}

// ── Result type ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct BenchResult {
    impl_name: String,
    threads: usize,
    ops_per_sec: f64,
    hit_rate_pct: f64,
    p50_ns: u64,
    p95_ns: u64,
    p99_ns: u64,
    p999_ns: u64,
}

// ── Measurement ───────────────────────────────────────────────────────────────

fn make_hist() -> Histogram<u64> {
    Histogram::<u64>::new_with_bounds(1, 10_000_000_000, 3).expect("valid HDR bounds")
}

fn run_bench(cache: Arc<dyn CacheBench>, num_threads: usize) -> BenchResult {
    // Pre-fill so reads have hits to measure.
    for i in 0..KEY_SPACE.min(CAPACITY as u64) {
        cache.do_push(i, i);
    }

    let total_ops = Arc::new(AtomicU64::new(0));
    let total_hits = Arc::new(AtomicU64::new(0));
    let deadline = Instant::now() + BENCH_DURATION;

    let thread_hists: Vec<Histogram<u64>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..num_threads)
            .map(|t| {
                let cache = Arc::clone(&cache);
                let total_ops = Arc::clone(&total_ops);
                let total_hits = Arc::clone(&total_hits);
                s.spawn(move || {
                    let mut hist = make_hist();
                    let mut ops = 0u64;
                    let mut hits = 0u64;
                    let mut i = 0u64;

                    while Instant::now() < deadline {
                        let key = (t as u64).wrapping_mul(1_000_003).wrapping_add(i) % KEY_SPACE;
                        let is_write = (i % 100) < WRITE_PCT as u64;

                        let t0 = Instant::now();
                        if is_write {
                            cache.do_push(key, key);
                        } else {
                            if cache.do_get(key) {
                                hits += 1;
                            }
                        }
                        let ns = t0.elapsed().as_nanos() as u64;
                        let _ = hist.record(ns.max(1));
                        ops += 1;
                        i += 1;
                    }

                    total_ops.fetch_add(ops, Ordering::Relaxed);
                    total_hits.fetch_add(hits, Ordering::Relaxed);
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
    for h in thread_hists {
        merged.add(&h).expect("histogram merge");
    }

    let total = total_ops.load(Ordering::Relaxed);
    let hits = total_hits.load(Ordering::Relaxed);
    let reads = (total as f64 * (1.0 - WRITE_PCT as f64 / 100.0)).max(1.0);
    let hit_pct = hits as f64 / reads * 100.0;

    BenchResult {
        impl_name: cache.impl_name().to_string(),
        threads: num_threads,
        ops_per_sec: total as f64 / BENCH_DURATION.as_secs_f64(),
        hit_rate_pct: hit_pct,
        p50_ns: merged.value_at_quantile(0.50),
        p95_ns: merged.value_at_quantile(0.95),
        p99_ns: merged.value_at_quantile(0.99),
        p999_ns: merged.value_at_quantile(0.999),
    }
}

// ── Console output ────────────────────────────────────────────────────────────

fn print_results(results: &[BenchResult]) {
    println!("\n{}", "═".repeat(110));
    println!("  LRU CACHE ANALYTICS REPORT");
    println!(
        "  Config: capacity={CAPACITY}, shards={DEFAULT_SHARDS}, \
         write%={WRITE_PCT}, key_space={KEY_SPACE}, duration={}s",
        BENCH_DURATION.as_secs()
    );
    println!("{}", "═".repeat(110));
    println!(
        "  {:<40}  {:>7}  {:>12}  {:>8}  {:>8}  {:>9}  {:>10}",
        "Implementation", "Threads", "Ops/sec", "Hit%", "p50(ns)", "p99(ns)", "p999(ns)"
    );
    println!("  {}", "─".repeat(106));

    for r in results {
        println!(
            "  {:<40}  {:>7}  {:>12.0}  {:>7.1}%  {:>8}  {:>9}  {:>10}",
            r.impl_name, r.threads, r.ops_per_sec, r.hit_rate_pct, r.p50_ns, r.p99_ns, r.p999_ns,
        );
    }
    println!("{}\n", "═".repeat(110));
}

// ── HTML report ───────────────────────────────────────────────────────────────

fn generate_html(results: &[BenchResult], path: &str) {
    let f = File::create(path).expect("cannot create report.html");
    let mut w = BufWriter::new(f);

    // Helper: collect results for a single thread count.
    let for_threads =
        |t: usize| -> Vec<&BenchResult> { results.iter().filter(|r| r.threads == t).collect() };

    // Build SVG bar chart for throughput or latency.
    // `values`: slice of (label, value)
    fn svg_bars(values: &[(&str, f64)], x_label: &str, color_base: &str) -> String {
        if values.is_empty() {
            return String::new();
        }
        let max_val = values.iter().map(|v| v.1).fold(0.0f64, f64::max).max(1.0);
        let bar_h = 28usize;
        let gap = 8usize;
        let label_w = 280usize;
        let chart_w = 380usize;
        let total_h = values.len() * (bar_h + gap) + 40;

        // Use r##"..."## so that # inside (hex colors) don't end the raw string.
        let mut svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" font-family="monospace" font-size="12">"##,
            label_w + chart_w + 60,
            total_h
        );

        let colors = [
            "#6366f1", "#22d3ee", "#f59e0b", "#34d399", "#f87171", "#a78bfa",
        ];
        for (i, (label, val)) in values.iter().enumerate() {
            let y = 20 + i * (bar_h + gap);
            let bar_px = ((*val / max_val) * chart_w as f64) as usize;
            let color = if color_base.is_empty() {
                colors[i % colors.len()]
            } else {
                color_base
            };
            // label
            svg.push_str(&format!(
                r##"<text x="{}" y="{}" fill="#cbd5e1" text-anchor="end">{}</text>"##,
                label_w - 6,
                y + bar_h - 8,
                label
            ));
            // bar
            svg.push_str(&format!(
                r##"<rect x="{}" y="{}" width="{}" height="{}" rx="4" fill="{}"/>"##,
                label_w, y, bar_px, bar_h, color
            ));
            // value text
            let val_str = if *val >= 1_000_000.0 {
                format!("{:.2}M", *val / 1_000_000.0)
            } else if *val >= 1_000.0 {
                format!("{:.1}K", *val / 1_000.0)
            } else {
                format!("{:.0}", *val)
            };
            svg.push_str(&format!(
                r##"<text x="{}" y="{}" fill="#94a3b8">{}</text>"##,
                label_w + bar_px + 6,
                y + bar_h - 8,
                val_str
            ));
        }
        svg.push_str(&format!(
            r##"<text x="{}" y="{}" fill="#64748b" font-size="11">{}</text>"##,
            label_w,
            total_h - 6,
            x_label
        ));
        svg.push_str("</svg>");
        svg
    }

    // Write HTML head + dark theme styles.
    write!(w, r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8"/>
<meta name="viewport" content="width=device-width,initial-scale=1"/>
<title>LRU Cache — Performance Report</title>
<style>
*{{box-sizing:border-box;margin:0;padding:0}}
body{{background:#0f172a;color:#e2e8f0;font-family:'JetBrains Mono',monospace,sans-serif;padding:2rem}}
h1{{font-size:1.8rem;font-weight:700;color:#f8fafc;margin-bottom:.4rem}}
.subtitle{{color:#94a3b8;margin-bottom:2rem;font-size:.95rem}}
h2{{font-size:1.1rem;font-weight:600;color:#c7d2fe;margin:2rem 0 .8rem;border-left:3px solid #6366f1;padding-left:.75rem}}
h3{{font-size:.95rem;color:#7dd3fc;margin:1.2rem 0 .5rem}}
.grid{{display:grid;grid-template-columns:repeat(auto-fit,minmax(420px,1fr));gap:1.5rem;margin-bottom:2rem}}
.card{{background:#1e293b;border:1px solid #334155;border-radius:.75rem;padding:1.25rem}}
table{{width:100%;border-collapse:collapse;font-size:.82rem}}
th{{background:#1e293b;color:#94a3b8;font-weight:600;text-align:left;padding:.45rem .6rem;border-bottom:1px solid #334155}}
td{{padding:.35rem .6rem;border-bottom:1px solid #1e293b;white-space:nowrap}}
tr:hover td{{background:#1e3a4a}}
.num{{text-align:right;font-variant-numeric:tabular-nums}}
.good{{color:#34d399}}.ok{{color:#fbbf24}}.bad{{color:#f87171}}
.badge{{display:inline-block;background:#312e81;color:#a5b4fc;border-radius:.3rem;padding:.1rem .4rem;font-size:.75rem;margin-right:.3rem}}
.cfg{{background:#0f172a;border:1px solid #334155;border-radius:.5rem;padding:.75rem 1rem;font-size:.8rem;color:#94a3b8;margin-bottom:1.5rem}}
.cfg span{{color:#7dd3fc}}
svg text{{font-family:'JetBrains Mono',monospace,sans-serif}}
.chart-wrap{{overflow-x:auto;padding:.5rem 0}}
footer{{margin-top:3rem;color:#475569;font-size:.78rem;text-align:center}}
</style>
</head>
<body>
<h1>🔒 Thread-Safe LRU Cache — Performance Report</h1>
<p class="subtitle">Generated by <code>cargo run --example report --release</code></p>
<div class="cfg">
  Config: <span>capacity={CAPACITY}</span> &nbsp;|&nbsp;
  shards=<span>{DEFAULT_SHARDS}</span> &nbsp;|&nbsp;
  write%=<span>{WRITE_PCT}%</span> &nbsp;|&nbsp;
  key_space=<span>{KEY_SPACE}</span> &nbsp;|&nbsp;
  duration=<span>{}s per run</span> &nbsp;|&nbsp;
  thread_counts=<span>{THREAD_COUNTS:?}</span>
</div>
"#, BENCH_DURATION.as_secs()).unwrap();

    // ── Section 1: Throughput summary table ──
    writeln!(w, "<h2>📊 Throughput Summary (ops/second)</h2>").unwrap();
    writeln!(w, r#"<div style="overflow-x:auto"><table>"#).unwrap();
    write!(w, "<tr><th>Implementation</th>").unwrap();
    for &t in THREAD_COUNTS {
        write!(w, "<th class='num'>{}T</th>", t).unwrap();
    }
    writeln!(w, "</tr>").unwrap();

    let impls = [
        "basic::ThreadSafeLru (global Mutex)",
        "sharded::Cache (LRU, 4 shards)",
        "sharded::Cache (FIFO, 4 shards)",
    ];
    for name in &impls {
        write!(w, "<tr><td><b>{}</b></td>", name).unwrap();
        for &t in THREAD_COUNTS {
            if let Some(r) = results
                .iter()
                .find(|r| r.impl_name == *name && r.threads == t)
            {
                let cls = if r.ops_per_sec > 5_000_000.0 {
                    "good"
                } else if r.ops_per_sec > 1_000_000.0 {
                    "ok"
                } else {
                    "bad"
                };
                write!(
                    w,
                    "<td class='num {}'>{:.2}M</td>",
                    cls,
                    r.ops_per_sec / 1_000_000.0
                )
                .unwrap();
            } else {
                write!(w, "<td>—</td>").unwrap();
            }
        }
        writeln!(w, "</tr>").unwrap();
    }
    writeln!(w, "</table></div>").unwrap();

    // ── Section 2: Throughput charts per thread count ──
    writeln!(w, "<h2>📈 Throughput Charts</h2><div class='grid'>").unwrap();
    for &t in THREAD_COUNTS {
        let rs = for_threads(t);
        let bars: Vec<(&str, f64)> = rs
            .iter()
            .map(|r| (r.impl_name.as_str(), r.ops_per_sec))
            .collect();
        let chart = svg_bars(&bars, "ops/sec", "");
        write!(
            w,
            r#"<div class="card"><h3>{} Thread(s)</h3><div class="chart-wrap">{}</div></div>"#,
            t, chart
        )
        .unwrap();
    }
    writeln!(w, "</div>").unwrap();

    // ── Section 3: Latency table ──
    writeln!(w, "<h2>⏱ Latency Percentiles (nanoseconds)</h2>").unwrap();
    writeln!(w, r#"<div style="overflow-x:auto"><table>"#).unwrap();
    writeln!(w, "<tr><th>Implementation</th><th class='num'>Threads</th><th class='num'>p50</th><th class='num'>p95</th><th class='num'>p99</th><th class='num'>p999</th></tr>").unwrap();
    for r in results {
        let cls99 = if r.p99_ns < 1_000 {
            "good"
        } else if r.p99_ns < 10_000 {
            "ok"
        } else {
            "bad"
        };
        writeln!(w, "<tr><td>{}</td><td class='num'>{}</td><td class='num'>{}</td><td class='num'>{}</td><td class='num {}'>{}</td><td class='num'>{}</td></tr>",
            r.impl_name, r.threads, r.p50_ns, r.p95_ns, cls99, r.p99_ns, r.p999_ns).unwrap();
    }
    writeln!(w, "</table></div>").unwrap();

    // ── Section 4: p99 latency charts ──
    writeln!(w, "<h2>🎯 p99 Latency Charts</h2><div class='grid'>").unwrap();
    for &t in THREAD_COUNTS {
        let rs = for_threads(t);
        let bars: Vec<(&str, f64)> = rs
            .iter()
            .map(|r| (r.impl_name.as_str(), r.p99_ns as f64))
            .collect();
        let chart = svg_bars(&bars, "p99 latency (ns)", "#f87171"); // color_base passed as str, safe
        write!(
            w,
            r#"<div class="card"><h3>{} Thread(s)</h3><div class="chart-wrap">{}</div></div>"#,
            t, chart
        )
        .unwrap();
    }
    writeln!(w, "</div>").unwrap();

    // ── Section 5: Hit rate table ──
    writeln!(w, "<h2>🎲 Cache Hit Rate</h2>").unwrap();
    writeln!(w, r#"<div style="overflow-x:auto"><table>"#).unwrap();
    writeln!(
        w,
        "<tr><th>Implementation</th><th class='num'>Threads</th><th class='num'>Hit Rate</th></tr>"
    )
    .unwrap();
    for r in results {
        let cls = if r.hit_rate_pct > 70.0 {
            "good"
        } else if r.hit_rate_pct > 40.0 {
            "ok"
        } else {
            "bad"
        };
        writeln!(
            w,
            "<tr><td>{}</td><td class='num'>{}</td><td class='num {}'>{:.1}%</td></tr>",
            r.impl_name, r.threads, cls, r.hit_rate_pct
        )
        .unwrap();
    }
    writeln!(w, "</table></div>").unwrap();

    // ── Section 6: Key takeaways ──
    writeln!(w, r#"<h2>📝 How to Run</h2>
<div class="card" style="font-size:.85rem;line-height:1.8">
<p><span class="badge">report</span>  <code>cargo run --example report --release</code>  — regenerate this HTML</p>
<p><span class="badge">compare</span> <code>cargo bench --bench compare</code>  — Criterion throughput charts (HTML in <code>target/criterion/</code>)</p>
<p><span class="badge">latency</span> <code>cargo bench --bench latency</code>  — HDR histogram latency CSV + table</p>
<p><span class="badge">baseline</span><code>cargo bench --bench compare -- --save-baseline main</code>  — save a baseline</p>
<p><span class="badge">diff</span>    <code>cargo bench --bench compare -- --load-baseline main --baseline main</code>  — compare against baseline</p>
<p><span class="badge">flamegraph</span><code>bash scripts/flamegraph.sh</code>  — generate per-impl SVG flamegraphs</p>
<p><span class="badge">all</span>     <code>bash scripts/run_all.sh</code>  — run everything in one shot</p>
</div>"#).unwrap();

    writeln!(
        w,
        r#"<footer>Generated at {} UTC &nbsp;·&nbsp; test-thread-safe-lru-cache</footer>
</body></html>"#,
        chrono_now()
    )
    .unwrap();

    drop(w);
}

fn chrono_now() -> String {
    // Simple wall-clock string without pulling in chrono.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (h, m, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
    format!("{:02}:{:02}:{:02}", h, m, s)
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() {
    println!("\n  ┌─────────────────────────────────────────────────┐");
    println!("  │  LRU Cache Analytics Report Generator            │");
    println!(
        "  │  Each run: {}s × {} thread configs              │",
        BENCH_DURATION.as_secs(),
        THREAD_COUNTS.len()
    );
    println!("  └─────────────────────────────────────────────────┘\n");

    let mut all_results: Vec<BenchResult> = Vec::new();

    for &threads in THREAD_COUNTS {
        println!(
            "  ── {} thread(s) ──────────────────────────────────",
            threads
        );

        // Basic
        print!("    [1/3] basic::ThreadSafeLru ...  ");
        let _ = std::io::stdout().flush();
        let cache: Arc<dyn CacheBench> = Arc::new(BasicLru(ThreadSafeLru::new(CAPACITY)));
        let r = run_bench(cache, threads);
        println!(
            "{:.2}M ops/sec  hit={:.1}%  p99={}ns",
            r.ops_per_sec / 1e6,
            r.hit_rate_pct,
            r.p99_ns
        );
        all_results.push(r);

        // Sharded LRU
        print!("    [2/3] sharded LRU ({} shards) ...  ", DEFAULT_SHARDS);
        let _ = std::io::stdout().flush();
        let cache: Arc<dyn CacheBench> = Arc::new(ShardedLru(Cache::lru(CAPACITY, DEFAULT_SHARDS)));
        let r = run_bench(cache, threads);
        println!(
            "{:.2}M ops/sec  hit={:.1}%  p99={}ns",
            r.ops_per_sec / 1e6,
            r.hit_rate_pct,
            r.p99_ns
        );
        all_results.push(r);

        // Sharded FIFO
        print!("    [3/3] sharded FIFO ({} shards) ... ", DEFAULT_SHARDS);
        let _ = std::io::stdout().flush();
        let cache: Arc<dyn CacheBench> =
            Arc::new(ShardedFifo(Cache::fifo(CAPACITY, DEFAULT_SHARDS)));
        let r = run_bench(cache, threads);
        println!(
            "{:.2}M ops/sec  hit={:.1}%  p99={}ns",
            r.ops_per_sec / 1e6,
            r.hit_rate_pct,
            r.p99_ns
        );
        all_results.push(r);
        println!();
    }

    print_results(&all_results);

    let html_path = "report.html";
    generate_html(&all_results, html_path);
    println!("  ✓  HTML report written → {}", html_path);
    println!(
        "     Open with: xdg-open {} (Linux)  /  open {} (macOS)\n",
        html_path, html_path
    );
}
