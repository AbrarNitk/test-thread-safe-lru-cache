use crate::sharded::eviction::{Eviction, lru::Lru};

use std::{
    hash::{BuildHasher, Hash, RandomState},
    marker::PhantomData,
};

// implemented eviction policy
pub enum EvictionPolicy {
    Lru,
    AsyncLru,
}

// cache is the container for the N number of shards
// for the provided eviction policy
// If there are N number of shards, that how it is going
// to divide the contention of threads
pub struct Cache<Key, Value, EP> {
    shards: Vec<EP>,
    hash_builder: RandomState,
    // marking that we are certainly going to use the generic params
    _phatntom: std::marker::PhantomData<(Key, Value)>,
}

impl<Key, Value, EP> Cache<Key, Value, EP>
where
    Key: Send + Sync + Clone + Eq + Hash,
    Value: Send + Sync + Clone,
    EP: Eviction<Key, Value>,
{
    // push the key to particular shard based on the key hash
    pub fn push(&self, k: Key, v: Value) {
        let shard_hash = self.hash_builder.hash_one(&k);
        let shard = (shard_hash as usize) % self.shards.len();
        self.shards[shard].push(k, v);
    }

    // get the key from particular shard
    pub fn get(&self, k: &Key) -> Option<Value> {
        let shard_hash = self.hash_builder.hash_one(k);
        let shard = (shard_hash as usize) % self.shards.len();
        self.shards[shard].get(k)
    }

    // check if the key does contain in it,s particular shard
    pub fn contains(&self, k: &Key) -> bool {
        let shard_hash = self.hash_builder.hash_one(k);
        let shard = (shard_hash as usize) % self.shards.len();
        self.shards[shard].contains(k)
    }

    // check if the cache is empty
    // visit each and check if all are empty
    pub fn is_empty(&self) -> bool {
        for s in self.shards.iter() {
            if !s.is_empty() {
                return false;
            }
        }
        true
    }

    // check the size of the cache
    // iterate all the shards and cumulate all of their sizes
    pub fn size(&self) -> usize {
        let mut size = 0;
        for s in self.shards.iter() {
            size += s.len();
        }
        size
    }
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
            hash_builder: RandomState::new(),
            _phatntom: PhantomData,
        }
    }

    // todo: this can be provided with async compilation flag as well
    pub fn lru_async() {
        todo!()
    }
}

#[cfg(test)]
mod lru_tests {

    use std::char;

    use super::*;
    use rand::{Rng, RngExt, distr::Alphanumeric};

    #[test]
    fn single_thread_push() {
        let cache = Cache::lru(100000, 10);

        for key in 0..1000000 {
            cache.push(key, key + 1);
        }

        // result: taking around a second
    }

    #[test]
    fn single_thread_push_get() {
        let cache = Cache::lru(100000, 10);

        // push
        for key in 0..1000000 {
            cache.push(key, key + 1);
        }

        // get
        for key in 0..1000000 {
            cache.get(&key);
        }
        // result: taking around a second
    }

    #[test]
    fn single_thread_push_get_string() {
        let cache = Cache::lru(100000, 10);

        let rng = rand::rng();

        let string_iter = std::iter::repeat_with(|| {
            rng.clone()
                .sample_iter(&Alphanumeric)
                .take(10)
                .map(char::from)
                .collect::<String>()
        });

        // push
        for key in string_iter.take(1000000) {
            cache.push(key.clone(), key);
        }

        // get
        for key in string_iter.take(1000000) {
            cache.get(&key);
        }
        // result: taking around 6 second
    }

    #[test]
    fn with_threads_push() {
        let cache = Cache::lru(100000, 10);
        std::thread::scope(|scope| {
            for _ in 0..10 {
                scope.spawn(|| {
                    for key in 0..1000000 {
                        cache.push(key, key);
                    }
                });
            }
        });
    }

    #[test]
    fn with_threads_push_and_get() {
        let cache = Cache::lru(1000000, 10);
        std::thread::scope(|scope| {
            for _ in 0..10 {
                scope.spawn(|| {
                    for key in 0..1000000 {
                        cache.push(key, key);
                    }
                });
            }
        });
    }
}

#[cfg(test)]
mod lru_async_tests {}
