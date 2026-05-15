pub mod lru;

// interface for the cache eviction policy algorithms
pub trait Eviction<Key, Value>: Send + Sync {
    fn new(capacity: usize) -> Self;
    fn pop(&mut self, key: &Key) -> Option<Value>;
    fn push(&mut self, key: Key, value: Value);
    fn remove(&mut self, key: &Key);
    fn contains(&self, key: &Key);
    fn len(&self);
    fn is_empty(&self);
}

pub trait EvictBuilder<Key, Value> {
    type Policy: Eviction<Key, Value>;
    fn build(&self, capacity: usize) -> Self::Policy;
}
