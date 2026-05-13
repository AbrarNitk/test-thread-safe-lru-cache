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

    pub fn move_to_front(&mut self, mut node_ptr: std::ptr::NonNull<Node<Key, Value>>) {
        // todo: SAFETY??
        let node = unsafe { node_ptr.as_mut() };

        // check if it is already at the front
        // note: node should be the part of the dll, means head should not be nullable
        if node.prev.is_none() {
            assert!(self.head.is_some(), "expected head to be present");
            return;
        }

        // if node is tail, just unlink the previous and move the tail to back
        if node.next.is_none() {
            if let Some(mut tail_prev) = node.prev {
                unsafe { tail_prev.as_mut().next = None };
                self.tail = Some(tail_prev);
            }
        } else {
            // unlink from prev if present
            if let Some(ref mut prev) = node.prev {
                // todo: SAFETY??
                unsafe { prev.as_mut().next = node.next };
            }

            // unlink from next
            if let Some(ref mut next) = node.next {
                // todo: SAFETY??
                unsafe { next.as_mut().prev = node.prev };
            }
        }

        // make sure to put the prev as null and next will overridden, and it is will always
        // be overridden, because at-least one more node must be present, if this executes
        node.prev = None;
        if let Some(mut current_head) = self.head {
            // todo: SAFETY??
            unsafe { current_head.as_mut().prev = Some(node_ptr) };
            node.next = Some(current_head);
            self.head = Some(node_ptr);
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
            println!("node is going out of the box running");
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

    #[test]
    fn move_front_test() {
        let mut dll = Dll::new();
        assert!(dll.is_empty(), "dll must be empty");

        let first = dll.push_front(1, 1);
        assert_eq!(dll.size(), 1, "dll-size: expected to have 1 size");

        let second = dll.push_front(2, 2);
        assert_eq!(dll.size(), 2, "dll-size: expected to have 2 size");

        dll.push_front(3, 3);
        assert_eq!(dll.size(), 3, "dll-size: expected to have 3 size");

        // after three insertion dll: 3 --> 2 --> 1

        dll.move_to_front(second);
        // after move front to sesond dll: 2 --> 3 --> 1
        assert_eq!(dll.size(), 3, "dll-size: expected to have 3 size");
        dll.move_to_front(first);
        // after move front to sesond dll: 1 --> 2 --> 3
        assert_eq!(dll.size(), 3, "dll-size: expected to have 3 size");

        let (k, _) = dll.pop_back().unwrap();
        assert_eq!(3, k);
        assert_eq!(dll.size(), 2, "dll-size: expected to have 2 size");

        let (k, _) = dll.pop_back().unwrap();
        assert_eq!(2, k);
        assert_eq!(dll.size(), 1, "dll-size: expected to have 1 size");

        let (k, _) = dll.pop_back().unwrap();
        assert_eq!(1, k);
        assert_eq!(dll.size(), 0, "dll-size: expected to have 0 size");
    }

    #[test]
    fn move_front_memory_leak_test() {
        let mut dll = Dll::new();
        assert!(dll.is_empty(), "dll must be empty");

        // check before drop and after drop impl: both cases are working as expected
        for i in 1..10000 {
            let node = dll.push_front(i, i);
            if i % 5 == 0 {
                dll.move_to_front(node);
            }
            assert_eq!(
                dll.size(),
                i,
                "{}",
                format!("dll-size: expected to have {i} size")
            );
        }
    }
}
