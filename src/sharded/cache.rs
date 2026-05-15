use crate::sharded::{
    cache,
    eviction::{EvictBuilder, Eviction, lru::Lru},
};

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
    fn new<Builder: Fn() -> EP>(total_shards: usize, shard_builder: Builder) -> Self {
        let mut shards = Vec::with_capacity(total_shards);
        for _ in 0..total_shards {
            let shard = shard_builder();
            shards.push(shard);
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
pub struct CacheBuilder<Key, Value, EP> {
    capacity: usize,
    shards: Option<usize>,
    policy: Option<EvictionPolicy>,
    // marking that we are certainly going to use the generic params
    _phantom: std::marker::PhantomData<(Key, Value, EP)>,
}

impl<Key, Value, EP> CacheBuilder<Key, Value, EP>
where
    Key: Send + Sync + Clone + Eq + Hash,
    Value: Send + Sync + Clone,
    EP: Eviction<Key, Value>,
{
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

    pub fn with_policy(mut self, policy: EvictionPolicy) -> Self {
        self.policy = Some(policy);
        self
    }

    pub fn build(self) -> Cache<Key, Value, EP> {
        let total_shards = self.shards.unwrap_or(4);

        create the builder in here
        let cache: Cache<Key, Value, EP> = match self.policy {
            Some(EvictionPolicy::Lru) | None => {
                let shard_builder = move || Lru::<Key, Value>::new(self.capacity);
                Cache::new(total_shards, shard_builder)
            }
            Some(EvictionPolicy::AsyncLru) => {
                let shard_builder = move || Lru::new(self.capacity);
                Cache::new(total_shards, shard_builder)
            }
        };
        cache

    }
}
