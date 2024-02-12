use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug)]
pub struct UnionFind<T> {
    parent: HashMap<T, T>,
}

impl<T: Clone + Eq + Hash> UnionFind<T> {
    pub fn new() -> Self {
        Self { parent: HashMap::new() }
    }

    pub fn get_root(&mut self, x: T) -> T {
        let p = self.parent.get(&x).unwrap_or(&x);
        if p == &x {
            x
        } else {
            let root = self.get_root(p.clone());
            self.parent.insert(x.clone(), root.clone());
            root
        }
    }

    pub fn union(&mut self, x: T, y: T) {
        let x = self.get_root(x);
        let y = self.get_root(y);
        self.parent.insert(x, y);
    }
}
