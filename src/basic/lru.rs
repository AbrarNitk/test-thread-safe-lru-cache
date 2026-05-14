use crate::basic::dll::{self, Node};
use std::{collections::HashMap, hash::Hash, ptr::NonNull};

pub struct Lru<Key, Value> {
    dll: dll::Dll<Key, Value>,
    map: HashMap<KeyRef<Key>, NonNull<Node<Key, Value>>>,
    size: usize,
    cap: usize,
}

// implement to that we store the pointer of key in the map
// and clone in not needed from the user to provide for the key
pub struct KeyRef<Key> {
    key_ref: *const Key,
}

impl<Key, Value> Lru<Key, Value> {
    pub fn new(cap: usize) -> Self {
        assert!(cap > 0, "lru capacity is expected to be grater than 0");
        Self {
            dll: dll::Dll::new(),
            map: HashMap::with_capacity(cap),
            size: 0,
            cap,
        }
    }

    pub fn push(&mut self, k: Key, v: Value)
    where
        Key: Eq + Hash,
    {
        let key_ref = KeyRef { key_ref: &k };

        // if key is available, then remove it from lru
        // because same has to be inserted again but it may different data
        if self.map.contains_key(&key_ref) {
            if let Some(node) = self.map.remove(&key_ref) {
                self.dll.remove(node);
                self.size -= 1;
            }
        }

        // if the size of the queue is exceeding capacity, remove the cold value from the last
        if self.size >= self.cap {
            if let Some((k, _v)) = self.dll.pop_back() {
                self.map.remove(&KeyRef { key_ref: &k });
                self.size -= 1;
            }
        }

        let node = self.dll.push_front(k, v);
        self.map.insert(key_ref, node);
        self.size += 1;
    }

    pub fn get(&mut self, key: &Key) -> Option<&Value>
    where
        Key: Eq + Hash,
    {
        match self.map.get(&KeyRef { key_ref: key }) {
            Some(node) => {
                self.dll.move_to_front(*node);
                unsafe { Some(&(*node.as_ptr()).value) }
            }
            None => None,
        }
    }

    pub fn size(&self) -> usize {
        assert_eq!(self.size, self.dll.size());
        self.size
    }

    pub fn is_empty(&self) -> bool {
        0 == self.size
    }
}

impl<Key: Hash> Hash for KeyRef<Key> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        unsafe { (*self.key_ref).hash(state) };
    }
}

impl<Key: Eq> Eq for KeyRef<Key> {}

impl<Key: Eq> PartialEq for KeyRef<Key> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { (*self.key_ref).eq(&*other.key_ref) }
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn create_test() {
        let lru = super::Lru::<i32, i32>::new(3);
        assert_eq!(lru.size(), 0);
        assert!(lru.is_empty());
    }

    #[test]
    fn push_test() {
        let mut lru = super::Lru::<String, i32>::new(3);
        lru.push("A".to_string(), 1);
        lru.push("B".to_string(), 2);
        lru.push("C".to_string(), 3);
        assert_eq!(lru.size(), 3);
        lru.push("D".to_string(), 4);
        assert_eq!(lru.size(), 3);
    }

    #[test]
    fn push_same_node_test() {
        let mut lru = super::Lru::<String, i32>::new(3);
        lru.push("A".to_string(), 1);
        lru.push("B".to_string(), 2);
        lru.push("C".to_string(), 3);
        lru.push("A".to_string(), 4);
        assert_eq!(lru.size(), 3);
        lru.push("D".to_string(), 5);
        assert_eq!(lru.size(), 3);
    }

    #[test]
    fn push_and_get_test() {
        let mut lru = super::Lru::<String, i32>::new(3);
        lru.push("A".to_string(), 1);
        assert_eq!(lru.size(), 1);
        lru.push("B".to_string(), 2);
        assert_eq!(lru.size(), 2);
        lru.push("C".to_string(), 3);
        assert_eq!(lru.size(), 3);
        lru.push("D".to_string(), 4);
        assert_eq!(lru.size(), 3);

        assert_eq!(lru.get(&"A".to_string()), None);
        assert_eq!(lru.get(&"D".to_string()).unwrap(), &4);
        assert_eq!(lru.get(&"B".to_string()).unwrap(), &2);

        lru.push("E".to_string(), 5);
        assert_eq!(lru.get(&"C".to_string()), None);
        assert_eq!(lru.size(), 3);
    }
}
