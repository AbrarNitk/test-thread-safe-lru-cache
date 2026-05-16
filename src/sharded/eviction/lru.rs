use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, Mutex, RwLock},
};

/// This module contains the code related to implementation of Lru Policy

// note: clone is cheap here
#[derive(Clone)]
pub struct Lru<Key, Value>
where
    Key: Send + Sync + Clone + Hash + Eq,
    Value: Send + Sync + Clone,
{
    // lru cache, maintain the nodes in an array and map that
    // contains key to index of the node in the array
    inner: Arc<RwLock<LruInner<Key, Value>>>,

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
impl<Key, Value> Lru<Key, Value>
where
    Key: Send + Sync + Clone + Hash + Eq,
    Value: Send + Sync + Clone,
{
    // push the provided node index at the front
    // caller of this api makes sure given index in available in the nodes
    // Note: from concurrent access, we are mostly safe in here because
    // we are asking caller to provide the mutable access to lru container
    fn push_front(inner: &mut LruInner<Key, Value>, node_index: usize) {
        let current_head_idx = inner.head;

        // if there is a head presents, point head.prev to the given node
        match current_head_idx {
            Some(head_idx) => {
                if let Some(current_head_node) = inner.nodes[head_idx].as_mut() {
                    current_head_node.prev = Some(node_index);
                }
            }
            None => {
                // if head not available then input node becomes the tail as well
                inner.tail = Some(node_index);
            }
        }

        // make input node to the new head
        let node = inner.nodes[node_index].as_mut().expect(
            "this is an assertaion which state that caller has to make sure the node is available",
        );
        node.next = current_head_idx;
        node.prev = None; // on the safer side, which make sure that prev is always point to none
        inner.head = Some(node_index);
    }

    // unlink the node, this api can be use to remove the node or push the node at the front
    // caller of this api has to make sure that provided node index is valid
    // Note: from concurrent access, we are mostly safe in here because
    // we are asking caller to provide the mutable access to lru container
    fn unlink_node(inner: &mut LruInner<Key, Value>, node_index: usize) {
        let (node_pre, node_next) = {
            let node = inner.nodes[node_index]
                .as_ref()
                .expect("this assertion makes sure that caller provides correct index");
            (node.prev, node.next)
        };

        // unlink from the previous
        match node_pre {
            // node is other than head node
            Some(pre_index) => {
                // SAFETY: unwrap is safe in here because if pre-index exists node must also exists
                inner.nodes[pre_index].as_mut().unwrap().next = node_next;
            }
            // node is head node
            None => {
                inner.head = node_next;
            }
        };

        // unlink from the next
        match node_next {
            // node is other than the tail
            Some(next_index) => {
                inner.nodes[next_index].as_mut().unwrap().prev = node_pre;
            }
            // node is tail node
            None => {
                inner.tail = node_pre;
            }
        };
    }

    // move the recently accessed nodes to the front iteratively as they have been added
    fn handle_recent_used(&self, inner: &mut LruInner<Key, Value>) {
        // todo: need to think about this lock, possibly we can use the parking_lot mutex in here for the light weight nature
        let mut recent_guard = self
            .recent_nodes_idx
            .lock()
            .expect("recency lock is poisoned");
        for &node_index in recent_guard.iter() {
            if node_index < inner.nodes.len() && inner.nodes[node_index].is_some() {
                // unlink the node where ever it is right now
                Self::unlink_node(inner, node_index);
                Self::push_front(inner, node_index);
            }
        }
        recent_guard.clear();
        drop(recent_guard);
    }

    // remove the node index from the LRU
    // Note: caller has to make sure that input index is available in the node array
    fn remove(inner: &mut LruInner<Key, Value>, node_index: usize) {
        // first unlink the node
        Self::unlink_node(inner, node_index);
        if let Some(node) = inner.nodes[node_index].take() {
            // mark the slot as free, so it can be used by others
            inner.available_slots.push(node_index);
            // remove the entry from the map, and we let overwritten the value
            inner.map.remove(&node.key);
        }
    }
}

/////////////////////////////
//// ## LRU Container ## ////
/////////////////////////////

pub struct LruNode<Key, Value> {
    key: Key,
    value: Value,
    prev: Option<usize>,
    next: Option<usize>,
}

// indexed based Lru
// it maintains the array of nodes(k,v,pre,next) and
// a map of key to index of the node in the array
pub struct LruInner<Key, Value> {
    // map of key to the index in the array
    map: HashMap<Key, usize>,

    // available nodes in the array
    nodes: Vec<Option<LruNode<Key, Value>>>,

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
        // inner is the rw-lockable
    }
    fn remove(&self, key: &Key) {
        todo!()
    }
    fn contains(&self, key: &Key) -> bool {
        todo!()
    }
    fn len(&self) -> usize {
        todo!()
    }
    fn is_empty(&self) -> bool {
        todo!()
    }
}
