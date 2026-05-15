pub mod lru;

// interface for the cache eviction policy algorithms
pub trait Eviction<Key, Value>: Send + Sync
where
    Key: Send + Sync + Clone + Eq + std::hash::Hash,
    Value: Send + Sync + Clone,
{
    fn new(capacity: usize) -> Self;
    fn pop(&mut self, key: &Key) -> Option<Value>;
    fn push(&mut self, key: Key, value: Value);
    fn remove(&mut self, key: &Key);
    fn contains(&self, key: &Key);
    fn len(&self);
    fn is_empty(&self);
}
