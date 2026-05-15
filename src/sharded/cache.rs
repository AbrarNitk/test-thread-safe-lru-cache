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
        let shard_capacity = capcacity / total_shards + 1;
        let mut shards = Vec::with_capacity(total_shards);
        for _ in 0..total_shards {
            let shard = shard_builder.build(shard_capacity);
            // this is an issue, because shard builder have its own type to return
            // and which is the bounded check, means there can be different implmentation with
            // the same bounds, which is not seems to be allowed, by the objects are allowed
            // which we do not want to keep to zero cost abstraction
            shards.push(shard as B::Policy as _);
        }

        Self {
            shards,
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
