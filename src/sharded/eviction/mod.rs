pub mod fifo;
pub mod lru;

// interface for the cache eviction policy algorithms
pub trait Eviction<Key, Value>: Send + Sync
where
    Key: Send + Sync + Clone + Eq + std::hash::Hash,
    Value: Send + Sync + Clone,
{
    fn new(capacity: usize) -> Self;
    fn get(&self, key: &Key) -> Option<Value>;
    fn push(&self, key: Key, value: Value);
    fn remove(&self, key: &Key);
    fn contains(&self, key: &Key) -> bool;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
}
