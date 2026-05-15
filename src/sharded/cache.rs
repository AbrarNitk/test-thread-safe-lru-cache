use crate::sharded::{
    cache,
    eviction::{Eviction, lru::Lru},
};

use std::{hash::Hash, marker::PhantomData};

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

// Cache implmentation for the Lru Policy
impl<Key, Value> Cache<Key, Value, Lru<Key, Value>>
where
    Key: Send + Sync + Clone + Eq + Hash,
    Value: Send + Sync + Clone,
{
    // build lru with the direct function call
    pub fn lru(capacity: usize, shards: usize) -> Self {
        assert!(
            shards > 0,
            "number of shards expected to be greater than zero"
        );
        let mut cache_shards = vec![];
        let capacity_per_shard = capacity / shards + 1;
        for _ in 0..shards {
            let shard = Lru::new(capacity_per_shard);
            cache_shards.push(shard);
        }

        Self {
            shards: cache_shards,
            _phatntom: PhantomData,
        }
    }

    // todo: this can be provided with compilation flag
    pub fn lru_async() {
        todo!()
    }
}
