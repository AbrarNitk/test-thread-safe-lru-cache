use parking_lot::{Mutex, RwLock};
use std::{collections::HashMap, hash::Hash, sync::Arc};

#[derive(Clone)]
pub struct Fifo<Key, Value>
where
    Key: Send + Sync + Clone + Hash + Eq,
    Value: Send + Sync + Clone,
{
    // lru cache, maintain the nodes in an array and map that
    // contains key to index of the node in the array
    inner: Arc<RwLock<FifoInner<Key, Value>>>,

    // Note: in general, reads are also mutable in LRU
    // but this mechanics provides that we many threads
    // can try reading at the same time, instead of
    // mutating the list each time we maintain the recent
    // accessed nodes in the array and insert of usize
    // into an array is fast instead of moving the node
    // to front at each time
    recent_nodes_idx: Arc<Mutex<Vec<usize>>>,

    // capacity of the lru
    capacity: usize,
}

/// LRU related utilities
impl<Key, Value> Fifo<Key, Value>
where
    Key: Send + Sync + Clone + Hash + Eq,
    Value: Send + Sync + Clone,
{
}

// ############################
// ##### FIFO Container ##### //
// ############################

pub struct FifoNode<Key, Value> {
    key: Key,
    value: Value,
    prev: Option<usize>,
    next: Option<usize>,
}

impl<Key, Value> FifoNode<Key, Value> {
    fn new(key: Key, value: Value) -> Self {
        Self {
            key,
            value,
            prev: None,
            next: None,
        }
    }
}

// indexed based Lru
// it maintains the array of nodes(k,v,pre,next) and
// a map of key to index of the node in the array
pub struct FifoInner<Key, Value> {
    // map of key to the index in the array
    map: HashMap<Key, usize>,

    // available nodes in the array
    nodes: Vec<Option<FifoNode<Key, Value>>>,

    // available places/index in the nodes array
    // Note: this helps us to easily figure out about
    // which place in the nodes is currently free to be used
    // instead of traversing the array each time
    available_slots: Vec<usize>,

    // head of the lru which points to the index of the array
    head: Option<usize>,

    // tail of the lru which points to the index of the array
    tail: Option<usize>,
}

impl<Key, Value> super::Eviction<Key, Value> for Fifo<Key, Value>
where
    Key: Send + Sync + Clone + Hash + Eq,
    Value: Send + Sync + Clone,
{
    fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(FifoInner {
                map: HashMap::default(),
                nodes: Vec::with_capacity(capacity),
                available_slots: Vec::with_capacity(capacity),
                head: None,
                tail: None,
            })),
            recent_nodes_idx: Arc::new(Mutex::new(Vec::with_capacity(1024))),
            capacity,
        }
    }

    fn get(&self, key: &Key) -> Option<Value> {
        todo!()
    }
    fn push(&self, key: Key, value: Value) {
        todo!()
    }
    fn remove(&self, key: &Key) {
        let mut _inner_guard = self.inner.write();
        if let Some(&_node_index) = _inner_guard.map.get(key) {
            todo!() // Self::remove(&mut inner_guard, node_index);
        }
    }
    fn contains(&self, key: &Key) -> bool {
        self.inner.read().map.contains_key(key)
    }
    fn len(&self) -> usize {
        self.inner.read().map.len()
    }
    fn is_empty(&self) -> bool {
        self.inner.read().map.is_empty()
    }
}
