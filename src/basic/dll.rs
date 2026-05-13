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
        match self.head {
            Some(mut current_head) => {
                unsafe {
                    current_head.as_mut().prev = Some(node_link_ptr);
                    node_link_ptr.as_mut().next = Some(current_head);
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

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        0 == self.size
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
}
