use parking_lot::{Mutex, RwLock};
use std::{borrow::Borrow, collections::HashMap, fmt::Debug, hash::Hash, sync::Arc};

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
        println!("  push-front: with index: {}", node_index);

        let current_head_idx = inner.head;

        // if there is a head presents, point head.prev to the given node
        match current_head_idx {
            Some(head_idx) => {
                // if let Some(current_head_node) = inner.nodes[head_idx].as_mut() {
                //     current_head_node.prev = Some(node_index);
                // }

                println!("{}", inner.nodes.len());
                println!("  current-head-index:  {head_idx}");
                inner.nodes[head_idx]
                    .as_mut()
                    .expect(&format!("head index node not found: {head_idx}"))
                    .prev = Some(node_index);
            }
            None => {
                // if head not available then input node becomes the tail as well
                println!("  changing the tail: {node_index}");
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

        println!("  front-push:done with index: {}", node_index);
    }

    // unlink the node, this api can be use to remove the node or push the node at the front
    // caller of this api has to make sure that provided node index is valid
    // Note: from concurrent access, we are mostly safe in here because
    // we are asking caller to provide the mutable access to lru container
    fn unlink_node(inner: &mut LruInner<Key, Value>, node_index: usize) {
        println!("  unlink-node: inside the unlink: {node_index}");
        println!(
            "    index: {}, node length: {} : {}",
            node_index,
            inner.nodes.len(),
            inner.nodes.get(node_index).unwrap().is_some()
        );

        let (node_pre, node_next) = {
            let node = inner.nodes[node_index].as_ref().expect(&format!(
                "this assertion makes sure that caller provides correct index: {node_index}"
            ));
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
        println!(
            " after-unlink: head => {:?}, tail: {:?}",
            inner.head, inner.tail
        );
        println!("  unlink done: {node_index}");
    }

    // move the recently accessed nodes to the front iteratively as they have been added
    fn handle_recent_used(&self, inner: &mut LruInner<Key, Value>) {
        // todo: need to think about this lock, possibly we can use the parking_lot mutex in here for the light weight nature
        let mut recent_guard = self.recent_nodes_idx.lock();
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
        println!("  inside remove: {node_index}");
        // first unlink the node
        println!("  unlink the node index: {node_index}");
        Self::unlink_node(inner, node_index);
        if let Some(node) = inner.nodes[node_index].take() {
            // remove the entry from the map, and we let overwritten the value
            inner.map.remove(&node.key);
            // mark the slot as free, so it can be used by others
            inner.available_slots.push(node_index);
        }
        println!("  remove done: {node_index}");
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

impl<Key, Value> LruNode<Key, Value> {
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
        // todo: what about to handle the ttl for node
        // now this opeation looks simple
        // if node is available in the map
        //  - add it to the recently used list instead of moving on each operation
        //  - at some limit we have to move the recent to front as well, we have to set the limit
        //  -
        // else
        //  - return simply None
        //
        todo!()
    }

    fn push(&self, key: Key, value: Value) {
        // take the write lock at the inner
        let mut inner_guard = self.inner.write();

        // recently used nodes which we are pushing in the recent-accessed
        // handle recent used so eviction policy works correctly
        // this makes a write a bit slow, and gives the read to boost
        // self.handle_recent_used(&mut inner_guard);

        // if the value is available already then update the value in place
        // other insert the value fresh
        match inner_guard.map.get(&key) {
            Some(&node_index) => {
                println!("inside the get");
                // Self::unlink_node(&mut inner_guard, node_index);
                // let node = inner_guard.nodes[node_index]
                //     .as_mut()
                //     .expect("assertion that if the index exists in then node exists");

                // // update the value in-place
                // node.value = value;

                // // push the node to the front of the lru
                // Self::push_front(&mut inner_guard, node_index);
            }
            // case if the node is not available in cache
            None => {
                println!("head: {:?}", inner_guard.head);
                println!("tail: {:?}", inner_guard.tail);

                // if capacity reached then make a room for a new node
                if inner_guard.map.len() >= self.capacity {
                    if let Some(tail_idx) = inner_guard.tail {
                        println!("remove tail because capacity is full tail-index: {tail_idx}");
                        Self::remove(&mut inner_guard, tail_idx);
                    }
                }

                // space in the node
                // - first check if indesx are available
                // - second: grab index from the nodes itself
                let node_index = match inner_guard.available_slots.pop() {
                    Some(index) => {
                        println!("index from available slot: {index}");
                        println!("len before insert {}", inner_guard.nodes.len());
                        inner_guard.nodes[index] = Some(LruNode::new(key.clone(), value));
                        println!("len after insert {}", inner_guard.nodes.len());
                        index
                    }
                    None => {
                        let index = inner_guard.nodes.len();
                        println!("index from len: {index}");
                        inner_guard
                            .nodes
                            .push(Some(LruNode::new(key.clone(), value)));
                        index
                    }
                };

                println!("pushed node at index: {}", node_index);
                inner_guard.map.insert(key, node_index);
                Self::push_front(&mut inner_guard, node_index);
                println!("head: {:?}", inner_guard.head);
                println!("tail: {:?}", inner_guard.tail);
            }
        }

        // Some more notes for concurrency
        // check if the key available in the map
        //  - update the value and move the node at the front
        // - if not available
        //   - check if enough space is not available then make room for the new node
        //   - remove the node from the tail, but before that we have to make sure that
        //     recent is clean otherwise it may evict the wrong node from the tail
        //   - then grab the free node from the list and then push the node at the from of it
        //   - insert the entry into the map
    }

    fn remove(&self, key: &Key) {
        todo!()
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

#[cfg(test)]
mod test {
    use crate::sharded::{cache::Cache, eviction::Eviction};

    use super::*;

    #[test]
    fn push_test() {
        let cache = Lru::new(2);
        assert_eq!(cache.len(), 0);
        // assert_eq!(cache.len(), 1);
        // assert!(cache.contains(&1));
        // assert_eq!(cache.contains(&2), false);

        println!("inside push: key: {}", 0);
        cache.push(0, 1);
        println!("push done: key: {}", 0);

        println!("-----------------------------");

        println!("inside push: key: {}", 1);
        cache.push(1, 2);
        println!("push done: key: {}", 1);

        println!("-----------------------------");

        println!("inside push: key: {}", 2);
        cache.push(2, 3);
        println!("push done: key: {}", 2);

        println!("-----------------------------");

        println!("inside push: key: {}", 3);
        cache.push(3, 4);
        println!("push done: key: {}", 3);

        println!("-----------------------------");
        cache.push(5, 1);
        cache.push(6, 1);
        cache.push(7, 1);
        cache.push(8, 1);

        cache.push(12, 1);
        cache.push(13, 1);
        cache.push(14, 1);
        cache.push(15, 1);
        cache.push(16, 1);
        cache.push(17, 1);
        cache.push(18, 1);

        println!("cache size: {}", cache.len());
    }
}
