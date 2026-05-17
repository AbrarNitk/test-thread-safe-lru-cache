use test_thread_safe_lru_cache::sharded::cache::Cache;

#[tokio::main]
async fn main() {
    with_async_example().await;
    with_example();
}

async fn with_async_example() {
    let cache = Cache::lru(1000, 8);
    cache.push("hi", "hello");
    cache.push("foo", "temp");
    assert_eq!(cache.get(&"hi"), Some("hello"));
}

fn with_example() {
    let cache = Cache::lru(1000, 8);
    cache.push("hi", "hello");
    cache.push("foo", "temp");
    assert_eq!(cache.get(&"hi"), Some("hello"));
}
