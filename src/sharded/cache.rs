use crate::sharded::eviction::{EvictBuilder, Eviction};

use std::hash::Hash;

// implemented eviction policy
pub enum EvictionPolicy {
    Lru,
    AsyncLru,
}

// cache is the container for the N number of shards
// for the provided eviction policy
pub struct Cache<Key, Value, EP>
where
    Key: Send + Sync + Clone + Eq + Hash,
    Value: Send + Sync + Clone,
    EP: Eviction<Key, Value>,
{
    shards: Vec<EP>,

    // marking that we are certainly going to use the generic params
    _phatntom: std::marker::PhantomData<(Key, Value)>,
}

impl<Key, Value, EP> Cache<Key, Value, EP>
where
    Key: Send + Sync + Clone + Eq + Hash,
    Value: Send + Sync + Clone,
    EP: Eviction<Key, Value>,
{
    fn new<B: EvictBuilder<Key, Value>>(
        capcacity: usize,
        total_shards: usize,
        shard_builder: B,
    ) -> Self {
        Self {
            shards: Vec::new(),
            _phatntom: std::marker::PhantomData,
        }
    }
}

/// Cache Builder
/// Default Evict Policy: LRU
/// Default Number of Shards: 16
pub struct CacheBuilder<Key, Value> {
    capacity: usize,
    shards: Option<usize>,
    policy: Option<EvictionPolicy>,
    // marking that we are certainly going to use the generic params
    _phantom: std::marker::PhantomData<(Key, Value)>,
}

impl<K, V> CacheBuilder<K, V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            shards: None,
            policy: None,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn with_shards(mut self, shards: usize) -> Self {
        self.shards = Some(shards);
        self
    }

    // pub fn build(self) -> Cache<K, V> {
    //     // create the builder in here
    //     let policy = match self.policy {};
    // }
}
