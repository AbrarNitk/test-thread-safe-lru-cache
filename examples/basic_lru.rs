use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

use hdrhistogram::Histogram;
use test_thread_safe_lru_cache::basic::safe::ThreadSafeLru;

fn main() {
    // cache config
    let capacity = 1024;
    let threads = 8;
    let ops_per_thread = 50_000;

    let cache = Arc::new(ThreadSafeLru::new(capacity));
    let barrier = Arc::new(Barrier::new(threads));
    let mut handles = Vec::new();

    // collect histograms per thread and then merge
    for t in 0..threads {
        let c = Arc::clone(&cache);
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            // fill the data in the cache for its capacity
            if t == 0 {
                for i in 0..capacity {
                    c.push(i, format!("warm-{}", i));
                }
            }

            // wait for all the threads to sync in here
            b.wait();

            // histogram for latencies in micros
            let mut hist = Histogram::<u64>::new_with_bounds(1, 10_000_000, 3).unwrap();

            for i in 0..ops_per_thread {
                let key = ((t * ops_per_thread) + i) % (capacity * 2);
                // latency for a combined push and get
                let start = Instant::now();
                c.push(key, format!("val-{}-{}", t, key));
                let _ = c.get(&key);
                let dur = start.elapsed();
                let micros = dur.as_micros().max(0) as u64;
                hist.record(micros).ok();
            }

            hist
        }));
    }

    // merge all the histograms from different thread
    let mut total = Histogram::<u64>::new_with_bounds(1, 10_000_000, 3).unwrap();
    for h in handles {
        let h = h.join().unwrap();
        total.add(&h).unwrap();
    }

    // summary of different percentiles
    println!("Total count: {}", total.len());
    println!("p50: {} us", total.value_at_quantile(0.50));
    println!("p90: {} us", total.value_at_quantile(0.90));
    println!("p99: {} us", total.value_at_quantile(0.99));
    println!("max: {} us", total.max());

    // write a CSV of percentile -> value (us)
    let file = File::create("latency_percentiles.csv").unwrap();
    let mut w = BufWriter::new(file);
    writeln!(w, "quantile,us").unwrap();
    for q in 0..100 {
        let quant = (q as f64) / 100.0;
        let v = total.value_at_quantile(quant);
        writeln!(w, "{:.4},{}", quant, v).unwrap();
    }
    println!("Wrote latency_percentiles.csv");
}
