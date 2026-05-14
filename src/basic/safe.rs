use crate::basic::lru;
use std::{
    hash::Hash,
    sync::{Arc, Mutex},
};

pub struct ThreadSafeLru<Key, Value> {
    lru: Mutex<lru::Lru<Key, Arc<Value>>>,
}

impl<Key, Value> ThreadSafeLru<Key, Value> {
    pub fn new(cap: usize) -> Self {
        Self {
            lru: Mutex::new(lru::Lru::new(cap)),
        }
    }

    pub fn push(&self, k: Key, v: Value)
    where
        Key: Eq + Hash,
    {
        let mut guard = self.lru.lock().unwrap();
        guard.push(k, Arc::new(v));
    }

    // that's okay to have an Arc here, any ways this seems to be reference to the value
    // now the pointer in the memory is thread safe, if the value is removed from
    // another thread so it will still be a valid pointer
    pub fn get(&self, key: &Key) -> Option<Arc<Value>>
    where
        Key: Eq + Hash,
    {
        let mut guard = self.lru.lock().unwrap();
        guard.get(key).cloned()
    }

    pub fn len(&self) -> usize {
        self.lru.lock().unwrap().size()
    }
}

unsafe impl<K: Send, V: Send + Sync> Send for ThreadSafeLru<K, V> {}
unsafe impl<K: Send, V: Send + Sync> Sync for ThreadSafeLru<K, V> {}

#[cfg(test)]
mod test {
    use super::*;
    use std::thread;
    #[test]
    fn test_concurrent_stress() {
        let capacity = 1000;
        let cache = Arc::new(ThreadSafeLru::new(capacity));
        let num_threads = 5;
        let ops_per_thread = 5000;

        thread::scope(|s| {
            for t in 0..num_threads {
                let cache = Arc::clone(&cache);

                s.spawn(move || {
                    for i in 0..ops_per_thread {
                        let key = (t * ops_per_thread) + i;
                        cache.push(key, key.to_string());
                        let _ = cache.get(&key);
                    }
                });
            }
        });
    }

    #[test]
    fn test_stress() {
        let capacity = 1000;
        let cache = Arc::new(ThreadSafeLru::new(capacity));
        let num_threads = 20;
        let ops_per_thread = 5000;

        for t in 0..num_threads {
            let key = t * ops_per_thread;
            cache.push(key, key.to_string());
            let _ = cache.get(&key);
        }
    }

    // repeatedly pushes the same small set of keys.
    #[test]
    fn test_concurrent_hot_keys() {
        let capacity = 16;
        let cache = Arc::new(ThreadSafeLru::new(capacity));
        let num_threads = 12;
        let ops_per_thread = 1_000;
        let hot_keys = [1usize, 2usize, 3usize, 4usize, 5usize];

        thread::scope(|s| {
            for _ in 0..num_threads {
                let cache = Arc::clone(&cache);
                s.spawn(move || {
                    for i in 0..ops_per_thread {
                        let k = hot_keys[i % hot_keys.len()];
                        cache.push(k, format!("hot-{}", k));

                        // alternate between get and push to exercise move-to-front + eviction
                        if i % 3 == 0 {
                            let _ = cache.get(&k);
                        }
                    }
                });
            }
        });

        assert!(cache.len() <= capacity);
    }
}
