use crate::sharded::eviction::Eviction;
use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, RwLock},
};

/// This module contains the code related to implementation of Lru Policy

// clone is cheap here
#[derive(Clone)]
pub struct Lru<Key, Value>
where
    Key: Send + Sync + Clone + Hash + Eq,
    Value: Send + Sync + Clone,
{
    inner: Arc<RwLock<LruInner<Key, Value>>>,
    capacity: usize,
}

pub struct LruNode<Key, Value> {
    key: Key,
    value: Value,
    prev: Option<u32>,
    next: Option<u32>,
}

pub struct LruInner<Key, Value> {
    map: HashMap<Key, u32>,
    nodes: Vec<Option<LruNode<Key, Value>>>,
    free_idx: Vec<u32>,
    head: Option<u32>,
    tail: Option<u32>,
}

impl<Key, Value> super::Eviction<Key, Value> for Lru<Key, Value>
where
    Key: Send + Sync + Clone + Hash + Eq,
    Value: Send + Sync + Clone,
{
    fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(LruInner {
                map: HashMap::default(),
                nodes: Vec::with_capacity(capacity),
                free_idx: Vec::with_capacity(capacity),
                head: None,
                tail: None,
            })),
            capacity,
        }
    }
    fn pop(&mut self, key: &Key) -> Option<Value> {
        todo!()
    }
    fn push(&mut self, key: Key, value: Value) {
        todo!()
    }
    fn remove(&mut self, key: &Key) {
        todo!()
    }
    fn contains(&self, key: &Key) {
        todo!()
    }
    fn len(&self) {
        todo!()
    }
    fn is_empty(&self) {
        todo!()
    }
}

pub struct LruBuilder {}

impl<Key, Value> super::EvictBuilder<Key, Value> for LruBuilder
where
    Key: Send + Sync + Clone + Hash + Eq,
    Value: Send + Sync + Clone,
{
    type Policy = Lru<Key, Value>;
    fn build(&self, capacity: usize) -> Self::Policy {
        Lru::new(capacity)
    }
}
