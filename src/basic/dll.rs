// this module contains the doubly linked list implementation for lru
// this is sppecifically non-thread safe

// represents the type in the linkedlist for previous and next node
pub type Link<Key, Value> = Option<std::ptr::NonNull<Node<Key, Value>>>;

// node container for doubly linkedlist
pub struct Node<Key, Value> {
    key: Key,
    value: Value,
    prev: Link<Key, Value>,
    next: Link<Key, Value>,
}

impl<Key, Value> Node<Key, Value> {
    fn new(key: Key, value: Value) -> Box<Self> {
        Box::new(Self {
            key,
            value,
            prev: None,
            next: None,
        })
    }
}

// doubly linkedlist container
pub struct Dll<Key, Value> {
    head: Link<Key, Value>,
    tail: Link<Key, Value>,
    size: usize,
}

impl<Key, Value> Default for Dll<Key, Value> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Key, Value> Dll<Key, Value> {
    pub fn new() -> Self {
        Self {
            head: None,
            tail: None,
            size: 0,
        }
    }

    pub fn push_front(&mut self, k: Key, v: Value) -> std::ptr::NonNull<Node<Key, Value>> {
        // boxed-node: heap allocated
        let node = Node::new(k, v);

        // SAFETY: we know that Box never returns the null pointer
        let mut node_link_ptr = unsafe { std::ptr::NonNull::new_unchecked(Box::into_raw(node)) };
        match self.head.as_mut() {
            Some(current_head) => {
                unsafe {
                    current_head.as_mut().prev = Some(node_link_ptr);
                    node_link_ptr.as_mut().next = Some(current_head.clone());
                }
                self.head = Some(node_link_ptr);
            }
            None => {
                self.head = Some(node_link_ptr);
                self.tail = self.head;
            }
        }
        self.size += 1;
        node_link_ptr
    }

    pub fn pop_back(&mut self) -> Option<(Key, Value)> {
        match self.tail.as_mut() {
            Some(current_tail) => {
                // SAFETY: we are sure that there is always going to valid pointer in the current-tail
                // because we are initializing it in the push-back function
                let mut tail_node = unsafe { Box::from_raw(current_tail.as_ptr()) };

                // unlinking tail from previous and point to the previous
                match tail_node.prev.as_mut() {
                    Some(tail_prev) => {
                        // unlinking the tail node from previous node if it exists
                        unsafe { tail_prev.as_mut().next = None };

                        // point the tail to the previous node
                        self.tail = Some(tail_prev.clone());
                    }
                    None => {
                        // otherwise this was the last node in the dll,
                        // hence point head and tail to nullable
                        self.head = None;
                        self.tail = None;
                    }
                }

                // decrease the size
                self.size -= 1;

                // we left out tail_node prev pointer, which is automatically dropped in here
                Some((tail_node.key, tail_node.value))
            }
            None => None,
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        0 == self.size
    }
}

impl<K, V> Drop for Dll<K, V> {
    fn drop(&mut self) {
        while !self.is_empty() {
            self.pop_back();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn push_and_size() {
        let mut dll = Dll::new();
        assert!(dll.is_empty(), "dll must be empty");

        dll.push_front(1, 1);
        assert_eq!(dll.size(), 1, "dll-size: expected to have 1 size");

        dll.push_front(2, 2);
        assert_eq!(dll.size(), 2, "dll-size: expected to have 2 size");
    }

    #[test]
    pub fn push_and_pop() {
        let mut dll = Dll::new();
        assert!(dll.is_empty(), "dll must be empty");

        dll.push_front(1, 1);
        assert_eq!(dll.size(), 1, "dll-size: expected to have 1 size");

        dll.push_front(2, 2);
        assert_eq!(dll.size(), 2, "dll-size: expected to have 2 size");

        dll.push_front(3, 3);
        assert_eq!(dll.size(), 3, "dll-size: expected to have 3 size");

        dll.pop_back();
        assert_eq!(dll.size(), 2, "dll-size: expected to have 2 size");

        dll.pop_back();
        assert_eq!(dll.size(), 1, "dll-size: expected to have 1 size");

        dll.pop_back();
        assert_eq!(dll.size(), 0, "dll-size: expected to have 0 size");
    }

    // checked with: `cargo valgrind test memory_leak_test`
    #[test]
    fn memory_leak_test() {
        let mut dll = Dll::new();
        assert!(dll.is_empty(), "dll must be empty");

        // check before drop and after drop impl: both cases are working as expected
        for i in 1..10000 {
            dll.push_front(i, i);
            assert_eq!(
                dll.size(),
                i,
                "{}",
                format!("dll-size: expected to have {i} size")
            );
        }
    }
}
