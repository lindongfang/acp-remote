//! Standard parent identity is separate from current activity and native resource ownership.
use super::DispatchError;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Default)]
pub(super) struct CapacityTree {
    parents: HashMap<Arc<str>, Arc<str>>,
    roots: HashSet<Arc<str>>,
}

impl CapacityTree {
    pub(super) fn root(&mut self, session: &str) -> Result<(), DispatchError> {
        if self.parents.contains_key(session) {
            return Err(DispatchError::SessionTreeUnverified);
        }
        self.roots.insert(Arc::from(session));
        Ok(())
    }

    pub(super) fn associate(&mut self, parent: &str, child: &str) -> Result<(), DispatchError> {
        if parent == child || self.roots.contains(child) {
            return Err(DispatchError::SessionTreeUnverified);
        }
        if let Some(original) = self.parents.get(child) {
            return if original.as_ref() == parent {
                Ok(())
            } else {
                Err(DispatchError::SessionTreeUnverified)
            };
        }
        let mut cursor = parent;
        let mut visited = HashSet::new();
        loop {
            if cursor == child || !visited.insert(cursor) {
                return Err(DispatchError::SessionTreeUnverified);
            }
            let Some(ancestor) = self.parents.get(cursor) else {
                break;
            };
            cursor = ancestor;
        }
        self.parents.insert(Arc::from(child), Arc::from(parent));
        Ok(())
    }

    pub(super) fn ancestors(&self, session: &str) -> Vec<Arc<str>> {
        let mut result = Vec::new();
        let mut cursor = session;
        while let Some(parent) = self.parents.get(cursor) {
            result.push(parent.clone());
            cursor = parent;
        }
        result
    }

    pub(super) fn members(&self, root: &str) -> Result<Vec<Arc<str>>, DispatchError> {
        if !self.roots.contains(root) || self.parents.contains_key(root) {
            return Err(DispatchError::SessionTreeUnverified);
        }
        let mut result = vec![Arc::from(root)];
        for child in self.parents.keys() {
            if self
                .ancestors(child)
                .last()
                .is_some_and(|last| last.as_ref() == root)
            {
                result.push(child.clone());
            }
        }
        result.sort();
        Ok(result)
    }

    pub(super) fn contains(&self, session: &str) -> bool {
        self.root_for(session).is_some()
    }

    pub(super) fn root_for(&self, session: &str) -> Option<Arc<str>> {
        if self.roots.contains(session) {
            return Some(Arc::from(session));
        }
        self.ancestors(session)
            .last()
            .filter(|root| self.roots.contains(*root))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_parent_is_not_a_verified_root() {
        let mut tree = CapacityTree::default();
        tree.associate("root", "child").unwrap();
        assert!(tree.members("root").is_err());
        tree.root("root").unwrap();
        assert_eq!(
            tree.members("root").unwrap(),
            vec![Arc::from("child"), Arc::from("root")]
        );
    }

    #[test]
    fn nested_identity_is_stable_and_conflicts_do_not_replace_it() {
        let mut tree = CapacityTree::default();
        tree.root("root").unwrap();
        tree.associate("root", "child").unwrap();
        tree.associate("child", "grandchild").unwrap();
        tree.associate("root", "child").unwrap();
        assert!(tree.associate("other", "child").is_err());
        assert!(tree.associate("grandchild", "root").is_err());
        assert!(tree.root("child").is_err());
        assert_eq!(
            tree.ancestors("grandchild"),
            vec![Arc::from("child"), Arc::from("root")]
        );
        assert_eq!(tree.members("root").unwrap().len(), 3);
    }

    #[test]
    fn incomplete_chain_cannot_cycle_before_a_root_is_known() {
        let mut tree = CapacityTree::default();
        tree.associate("a", "b").unwrap();
        assert!(tree.associate("b", "a").is_err());
        assert!(tree.associate("x", "x").is_err());
        assert!(!tree.contains("x"));
        assert_eq!(tree.ancestors("b"), vec![Arc::from("a")]);
    }
}
