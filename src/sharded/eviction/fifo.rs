use parking_lot::RwLock;
use std::{collections::HashMap, hash::Hash, sync::Arc};

#[derive(Clone)]
pub struct Fifo<Key, Value>
where
    Key: Send + Sync + Clone + Hash + Eq,
    Value: Send + Sync + Clone,
{
    // fifo cache, maintain the nodes in an array and map that
    // contains key to index of the node in the array
    inner: Arc<RwLock<FifoInner<Key, Value>>>,

    // capacity of the fifo
    capacity: usize,
}

impl<Key, Value> Fifo<Key, Value>
where
    Key: Send + Sync + Clone + Hash + Eq,
    Value: Send + Sync + Clone,
{
    fn push_back(inner: &mut FifoInner<Key, Value>, node_index: usize) {
        let current_tail = inner.tail;
        if let Some(tail_index) = current_tail {
            inner.nodes[tail_index]
                .as_mut()
                .unwrap_or_else(|| {
                    panic!("assertion which state that caller must pass the correct {node_index}")
                })
                .next = Some(node_index);
        } else {
            inner.head = Some(node_index);
        }

        let node = inner.nodes[node_index].as_mut().unwrap();
        inner.tail = Some(node_index);
        node.prev = current_tail;
        node.next = None;
    }

    // remove the node index from the Fifo
    // Note: caller has to make sure that input index is available in the node array
    fn remove(inner: &mut FifoInner<Key, Value>, node_index: usize) {
        Self::unlink_node(inner, node_index);
        if let Some(node) = inner.nodes[node_index].take() {
            inner.map.remove(&node.key);
            inner.available_slots.push(node_index);
        }
    }

    // unlink the node, this api can be use to remove the node or push the node at the back
    // caller of this api has to make sure that provided node index is valid
    // Note: from concurrent access, we are mostly safe in here because
    // we are asking caller to provide the mutable access to lru container
    fn unlink_node(inner: &mut FifoInner<Key, Value>, node_index: usize) {
        let (node_pre, node_next) = {
            let node = inner.nodes[node_index].as_ref().unwrap_or_else(|| {
                panic!("assertion which state that caller provides correct index: {node_index}")
            });
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
            capacity,
        }
    }

    fn get(&self, key: &Key) -> Option<Value> {
        let inner_guard = self.inner.read();

        if let Some(&node_index) = inner_guard.map.get(key) {
            let node = inner_guard.nodes[node_index]
                .as_ref()
                .expect("must be present");
            return Some(node.value.clone());
        }

        None
    }

    fn push(&self, key: Key, value: Value) {
        let mut inner_guard = self.inner.write();

        // if the key is available then update the value of the node
        if let Some(&node_index) = inner_guard.map.get(&key) {
            Self::unlink_node(&mut inner_guard, node_index);
            let node = inner_guard.nodes[node_index]
                .as_mut()
                .expect("assertion that if the index exists in then node exists");
            // update the value in-place
            node.value = value;
            // push the node to the front of the lru
            Self::push_back(&mut inner_guard, node_index);
            return;
        }

        // otherwise, make a room to insert the new node if it reaches to the capacity
        if inner_guard.map.len() >= self.capacity
            && let Some(node_index) = inner_guard.head
        {
            Self::remove(&mut inner_guard, node_index);
        }

        // then insert the node in the array at the tail
        let node_index = if let Some(idx) = inner_guard.available_slots.pop() {
            // if the place is already claimed in the array
            inner_guard.nodes[idx as usize] = Some(FifoNode::new(key.clone(), value));
            idx
        } else {
            // otherwise claim the place in the array
            let node_index = inner_guard.nodes.len();
            inner_guard
                .nodes
                .push(Some(FifoNode::new(key.clone(), value)));
            node_index
        };

        inner_guard.map.insert(key, node_index);
        Self::push_back(&mut inner_guard, node_index);
    }

    fn remove(&self, key: &Key) {
        let mut inner_guard = self.inner.write();
        if let Some(&node_index) = inner_guard.map.get(key) {
            Self::remove(&mut inner_guard, node_index);
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

#[cfg(test)]
mod test {
    use crate::sharded::eviction::Eviction;

    use super::*;

    #[test]
    fn push_test() {
        let cache = Fifo::new(10);
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

        assert_eq!(cache.len(), 10, "cache size error");
    }

    #[test]
    fn push_test_with_repeat_key() {
        let cache = Fifo::new(5);
        assert_eq!(cache.len(), 0);

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

        // repeat the keys again
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

        assert_eq!(cache.len(), 5, "cache size error");
    }

    #[test]
    fn push_and_get_test() {
        let cache = Fifo::new(10);
        assert_eq!(cache.len(), 0);

        cache.push(0, 1);
        cache.push(1, 2);
        cache.push(2, 3);
        cache.push(3, 4);
        cache.push(4, 5);
        cache.push(5, 6);
        cache.push(6, 7);
        cache.push(7, 8);
        cache.push(8, 9);
        cache.push(9, 10);
        assert_eq!(cache.len(), 10);

        assert_eq!(cache.get(&0), Some(1));
        assert_eq!(cache.get(&1), Some(2));
        assert_eq!(cache.get(&2), Some(3));
        assert_eq!(cache.get(&3), Some(4));
        assert_eq!(cache.get(&4), Some(5));
        assert_eq!(cache.get(&5), Some(6));
        assert_eq!(cache.get(&6), Some(7));
        assert_eq!(cache.get(&7), Some(8));
        assert_eq!(cache.get(&8), Some(9));
        assert_eq!(cache.get(&9), Some(10));
        assert_eq!(cache.len(), 10);

        cache.push(10, 11);
        cache.push(11, 12);
        cache.push(12, 13);
        cache.push(13, 14);
        cache.push(14, 15);
        cache.push(15, 16);
        cache.push(16, 17);
        cache.push(17, 18);
        cache.push(18, 19);
        cache.push(19, 20);

        // all are not availale now
        assert_eq!(cache.get(&0), None);
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&2), None);
        assert_eq!(cache.get(&3), None);
        assert_eq!(cache.get(&4), None);
        assert_eq!(cache.get(&5), None);
        assert_eq!(cache.get(&6), None);
        assert_eq!(cache.get(&7), None);
        assert_eq!(cache.get(&8), None);
        assert_eq!(cache.get(&9), None);
        assert_eq!(cache.len(), 10);

        // all are not availble
        assert_eq!(cache.get(&10), Some(11));
        assert_eq!(cache.get(&11), Some(12));
        assert_eq!(cache.get(&12), Some(13));
        assert_eq!(cache.get(&13), Some(14));
        assert_eq!(cache.get(&14), Some(15));
        assert_eq!(cache.get(&15), Some(16));
        assert_eq!(cache.get(&16), Some(17));
        assert_eq!(cache.get(&17), Some(18));
        assert_eq!(cache.get(&18), Some(19));
        assert_eq!(cache.get(&19), Some(20));

        assert_eq!(cache.len(), 10, "cache size error");
    }
}
