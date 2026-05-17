# Thread Safe Cache Implementation

I have tried to implement two versions of cache, doubly-linkedlist with hashmap based lru
and sharded cache with lru and fifo policy.

In the first DLL based when it comes to thread safe, used single Mutex, means all the operations
gets serialized, and used the unsafe part of the Rust with the safety guarentees.


## Approach 1: DLL + Map: Thread Safe Lru Cache

### Overview

Think of the cache as a `queue of sticky notes` on a whiteboard, ordered from
`most recently used` (front) to `least recently used` (back). When you use a
note, you move it to the front. When the board is full and you need to add a
new note, you erase the one at the very back (the least recently used).

### Data Structures

#### 1. Doubly Linked List (DLL)

Every cached item lives as a `heap-allocated node` (`Box<Node<K,V>>`) that
holds:

```text
  [ key | value | prev_ptr | next_ptr ]
```

- `prev` points to the node that was used more recently.
- `next` points to the node that was used less recently.
- The `head` of the list: most recently used item.
- The `tail` of the list: least recently used item (eviction candidate).

The DLL gives us `O(1) insert at front`, `O(1) remove from back`, and
`O(1) move-any-node-to-front` with only rewrite of the pointers.


#### 2. HashMap: Contains the Addresses Of Nodes

```
HashMap<KeyRef<K>, NonNull<Node<K,V>>>
```

The map stores a `raw pointer to each node` keyed by its key. This gives us
`O(1) lookup`: given a key, we instantly know exactly where in memory that
node lives.


### How `push` Works (step by step)

```text
push("B", 42)
```

- If key already exists: update the data of the node.
- If Cache full: make a room for new node, means remove the old one.
- Insert at front: allocates a node on the heap and rewires the head pointers.
- Update the map by storing a key reference then raw pointer to that node.


### How `get` Works (step by step)

```
get(&"B")
```

- Look up the key in the HashMap -> get the raw node pointer.
- move node at the front on the DLL, mostly pointer rewiring.
- Return Arc reference of the value.


### Thread Safety

The thread-safe wrapper (`safe::ThreadSafeLru`) wraps the entire `Lru` in a
single `std::sync::Mutex`:

```rust
pub struct ThreadSafeLru<K, V> {
    lru: Mutex<Lru<K, Arc<V>>>,
}
```


### Trade-offs

| Property | Result |
|----------|--------|
| Correctness | Guaranteed: single lock, total serialization |
| Simplicity | Very simple: no tricky concurrent logic |
| Single-thread throughput | Good: O(1) for get and push |
| Multi-thread throughput | Poor: every thread blocks every other thread |
| Read scalability | None: even reads are exclusive (LRU `get` is mutable) |



### Known Limitation

Because LRU's `get` must move the node to the front, `reads are as expensive
as writes` under the mutex lock. With many threads all doing reads, they all serialize
on the same mutex and this is the bottleneck that the sharded implementation
solves.



## Approach 2: Shared Index Based Thread Safe Cache

### Overview

The core idea: instead of one big lock for the whole cache, `split the cache
into N independent shards`. Each shard is its own mini-cache with its own
lock. A key always goes to the same shard find by its hash, so threads
working on different keys don't block each other at all.

The more shards, the less contention threads hitting different shards run
fully in parallel.

In this implementation, I have kept policies separate from mechanism and which does
makes sense in way that mechanism care about the logic(how should it happen), and
policies care about the rules, what should happen.


### Per-Shard Design: Index-Based Node Array

Instead of heap-allocated `Box<Node>` raw-pointers (as in the basic DLL impl),
each shard uses a `flat pre-allocated array` of nodes:

```text
  nodes:  [ Node0 | Node1 | Node2 | None | Node4 | ... ]
  map:    { key_A --> 0,  key_B --> 2,  key_C --> 4 }
  head:   4   (most recently used index)
  tail:   0   (eviction candidate index)
  free:   [3] (available slots stack)
```

- `nodes[i]` — stores `(key, value, prev_idx, next_idx)`, linked by integer
  indices instead of raw pointers.
- `map` — maps a key to its array index for O(1) lookup.
- `available_slots` — a stack of freed indices; evicted slots are reused
  without any new heap allocation.

This is more CPU-cache friendly than scattered heap nodes and avoids the
`unsafe` raw-pointer arithmetic of the basic impl.



### Locking Strategy

Each shard wraps its inner cache state in a `parking_lot::RwLock`:

- `parking_lot` is faster than `std::sync` (smaller overhead, fairer wakeups).
- `Multiple readers` can hold a shard's lock simultaneously (shared read).
- `One writer` gets exclusive access (blocks all other readers/writers of
  that shard only, not the whole cache).


### Deferred Recency Trick — LRU only

LRU's problem: `get` must move the accessed node to the front (a write). If
`get` always took the write lock, reads would be just as slow as in the basic impl.


- Solution: batch the promotions.


```text
get(key):
  1. Take READ lock --> clone value, push node_index into recent_buf
  2. Drop read lock
  3. recent_buf >= 64 entries?
       --> Take WRITE lock → move all buffered nodes to front at once
```

`recent_buf` is a tiny `Mutex<Vec<usize>>` — very cheap. Most reads never
touch the write lock at all. On every `push` (which needs a write lock anyway),
the buffer is flushed first so eviction always picks the correct LRU tail.



### Eviction Policies

Both LRU and FIFO implement the same `Eviction<K,V>` trait so the `Cache`
struct is completely policy-agnostic:

| Policy | Evicts | `get` lock cost | Best for |
|--------|--------|-----------------|----------|
| LRU | Least recently *used* | Read + tiny Mutex (batched write) | Workloads with temporal locality |
| FIFO | Oldest *inserted* | Read lock only | Uniform-access workloads |


### Trade-offs vs Basic Impl

| Property | Basic (global Mutex) | Sharded (RwLock × N) |
|----------|:--------------------:|:--------------------:|
| Read throughput | All threads serialize | Scales with N shards |
| Write throughput | All threads serialize | Scales with N shards |
| Hot-key contention | All threads, one lock | All threads, one shard |
| `unsafe` usage | Yes (raw pointer/KeyRef) | No (index-based) |
| Complexity | Simple | Moderate |

### Known Limitation

If many threads hit the `same key` (hot-key pattern), they all route to the
same shard and contend on that one lock. Sharding only helps when traffic is
spread across many keys.



## Tools Used

- miri
- valgrind
- flamegraph
- criterion
- and others


## References

- [Cache Consious Structure Layout](https://dl.acm.org/doi/epdf/10.1145/301618.301633)
- [LRU-K Page Replacement Algo](https://dl.acm.org/doi/pdf/10.1145/300515.300518)
- [OSTEP On Virtual Memory Management](https://pages.cs.wisc.edu/~remzi/OSTEP/)
- Gemini AI: for reports and help in documents and benches.
- others blogs references


## Run Tests

```shell
cargo test --all
```


## Run Benches and Generate Report

Please open the `reports.html` file to the results and check the directory
for the flamegraphs.


```shell
bash scripts/run_all.sh
```

Note: This script will take sometime to run the benches and generate the reports.


### What it does:

- Checks prerequisites (Rust toolchain)
- Runs `cargo bench --bench compare`  (Criterion throughput, saves baseline)
- Runs `cargo bench --bench latency`  (HDR histogram latency + CSV)
- Builds & runs examples/report.rs  (HTML analytics report)
- Opens report.html in the default browser
