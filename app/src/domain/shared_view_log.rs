//! Append-only indexed metadata with fixed-size lock scopes and no total entry budget.
use super::shared_view::ViewError;
use std::sync::{Arc, Mutex, RwLock};

// Fanout is a lock/allocation processing unit. A full root grows another level.
const FANOUT: usize = 64;
enum Node<T> {
    Leaf(Mutex<Vec<Arc<T>>>),
    Branch(Mutex<Vec<Arc<Self>>>),
}
struct Root<T> {
    node: Arc<Node<T>>,
    height: u32,
    len: usize,
    capacity: usize,
}
pub struct AppendLog<T> {
    root: RwLock<Root<T>>,
}
pub struct LogSnapshot<T> {
    node: Arc<Node<T>>,
    height: u32,
    len: usize,
}
impl<T> Clone for LogSnapshot<T> {
    fn clone(&self) -> Self {
        Self {
            node: Arc::clone(&self.node),
            height: self.height,
            len: self.len,
        }
    }
}
impl<T> Node<T> {
    fn leaf() -> Result<Arc<Self>, ViewError> {
        let mut items = Vec::new();
        items
            .try_reserve_exact(FANOUT)
            .map_err(|_| ViewError::AllocationFailure)?;
        Ok(Arc::new(Self::Leaf(Mutex::new(items))))
    }
    fn branch() -> Result<Arc<Self>, ViewError> {
        let mut children = Vec::new();
        children
            .try_reserve_exact(FANOUT)
            .map_err(|_| ViewError::AllocationFailure)?;
        Ok(Arc::new(Self::Branch(Mutex::new(children))))
    }
}
impl<T> AppendLog<T> {
    pub(crate) fn new() -> Result<Self, ViewError> {
        Ok(Self {
            root: RwLock::new(Root {
                node: Node::leaf()?,
                height: 0,
                len: 0,
                capacity: FANOUT,
            }),
        })
    }
    pub(crate) fn snapshot(&self) -> Result<LogSnapshot<T>, ViewError> {
        let root = self.root.read().map_err(|_| ViewError::ReadFailure)?;
        Ok(LogSnapshot {
            node: Arc::clone(&root.node),
            height: root.height,
            len: root.len,
        })
    }
    /// One worker owns append. Readers only borrow already published positions.
    pub(crate) fn append(&mut self, value: T) -> Result<usize, ViewError> {
        let root = self.root.get_mut().map_err(|_| ViewError::ReadFailure)?;
        let next = root
            .len
            .checked_add(1)
            .ok_or(ViewError::SequenceExhausted)?;
        if root.len == root.capacity {
            let capacity = root
                .capacity
                .checked_mul(FANOUT)
                .ok_or(ViewError::SequenceExhausted)?;
            let height = root
                .height
                .checked_add(1)
                .ok_or(ViewError::SequenceExhausted)?;
            let node = Node::branch()?;
            if let Node::Branch(children) = node.as_ref() {
                children
                    .lock()
                    .map_err(|_| ViewError::ReadFailure)?
                    .push(Arc::clone(&root.node));
            }
            root.node = node;
            root.capacity = capacity;
            root.height = height;
        }
        insert(
            &root.node,
            root.height,
            root.capacity,
            root.len,
            Arc::new(value),
        )?;
        let index = root.len;
        root.len = next;
        Ok(index)
    }
}
fn insert<T>(
    node: &Arc<Node<T>>,
    height: u32,
    capacity: usize,
    index: usize,
    value: Arc<T>,
) -> Result<(), ViewError> {
    if height == 0 {
        let Node::Leaf(items) = node.as_ref() else {
            return Err(ViewError::ReadFailure);
        };
        let mut items = items.lock().map_err(|_| ViewError::ReadFailure)?;
        if items.len() != index || items.len() >= FANOUT {
            return Err(ViewError::InvalidCursor);
        }
        items.push(value);
        drop(items);
        return Ok(());
    }
    let child_capacity = capacity / FANOUT;
    let child_index = index / child_capacity;
    let position = index % child_capacity;
    let Node::Branch(children) = node.as_ref() else {
        return Err(ViewError::ReadFailure);
    };
    let existing = {
        let children = children.lock().map_err(|_| ViewError::ReadFailure)?;
        children.get(child_index).cloned()
    };
    let child = if let Some(child) = existing {
        child
    } else {
        let child = if height == 1 {
            Node::leaf()?
        } else {
            Node::branch()?
        };
        let mut children = children.lock().map_err(|_| ViewError::ReadFailure)?;
        if children.len() != child_index || children.len() >= FANOUT {
            return Err(ViewError::InvalidCursor);
        }
        children.push(Arc::clone(&child));
        drop(children);
        child
    };
    insert(&child, height - 1, child_capacity, position, value)
}
impl<T> LogSnapshot<T> {
    pub(crate) const fn len(&self) -> usize {
        self.len
    }
    pub(crate) fn get(&self, index: usize) -> Result<Arc<T>, ViewError> {
        if index >= self.len {
            return Err(ViewError::InvalidCursor);
        }
        let mut node = Arc::clone(&self.node);
        let mut height = self.height;
        let mut capacity = FANOUT
            .checked_pow(height.checked_add(1).ok_or(ViewError::SequenceExhausted)?)
            .ok_or(ViewError::SequenceExhausted)?;
        let mut position = index;
        while height > 0 {
            let child_capacity = capacity / FANOUT;
            let child_index = position / child_capacity;
            position %= child_capacity;
            let Node::Branch(children) = node.as_ref() else {
                return Err(ViewError::ReadFailure);
            };
            let child = children
                .lock()
                .map_err(|_| ViewError::ReadFailure)?
                .get(child_index)
                .cloned()
                .ok_or(ViewError::InvalidCursor)?;
            node = child;
            height -= 1;
            capacity = child_capacity;
        }
        let Node::Leaf(items) = node.as_ref() else {
            return Err(ViewError::ReadFailure);
        };
        let item = items
            .lock()
            .map_err(|_| ViewError::ReadFailure)?
            .get(position)
            .cloned()
            .ok_or(ViewError::InvalidCursor)?;
        Ok(item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_view_append_log_grows_past_old_record_budgets_without_changing_old_snapshot() {
        let mut log = AppendLog::new().expect("fallible log");
        for n in 0..65 {
            assert_eq!(log.append(n).expect("append"), n);
        }
        let old = log.snapshot().expect("fixed H");
        for n in 65..32769 {
            assert_eq!(log.append(n).expect("no total record limit"), n);
        }
        let current = log.snapshot().expect("current");
        assert_eq!(old.len(), 65);
        assert_eq!(current.len(), 32769);
        for n in 0..current.len() {
            assert_eq!(*current.get(n).expect("indexed original position"), n);
        }
        for n in 0..old.len() {
            assert_eq!(*old.get(n).expect("original snapshot"), n);
        }
        assert!(matches!(old.get(65), Err(ViewError::InvalidCursor)));
    }
}
