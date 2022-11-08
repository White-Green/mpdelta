pub(super) trait CloneableIteratorMarker: Iterator {
    fn clone_dyn(&self) -> CloneableIterator<Self::Item>;
}

pub(super) struct CloneableIterator<T>(Box<dyn CloneableIteratorMarker<Item = T> + Send + Sync + 'static>);

impl<T: Iterator + Clone + Send + Sync + 'static> CloneableIteratorMarker for T {
    fn clone_dyn(&self) -> CloneableIterator<T::Item> {
        CloneableIterator(Box::new(self.clone()))
    }
}

impl<T> Clone for CloneableIterator<T> {
    fn clone(&self) -> Self {
        <dyn CloneableIteratorMarker<Item = T> + Send + Sync + 'static as CloneableIteratorMarker>::clone_dyn(&*self.0)
    }
}

impl<T> Iterator for CloneableIterator<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<T> CloneableIterator<T> {
    // TODO: trait-upcasting(https://github.com/rust-lang/rust/issues/65991)がstabilizeしたら直接中身を返すように変える
    pub(super) fn as_dyn_iterator(&mut self) -> &mut dyn Iterator<Item = T> {
        &mut *self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cloneable_iterator() {
        let mut iter = CloneableIterator(Box::new(0..10) as Box<dyn CloneableIteratorMarker<Item = i32> + Send + Sync + 'static>);
        assert_eq!(iter.next().unwrap(), 0);
        assert_eq!(iter.next().unwrap(), 1);
        let iter2 = iter.clone();
        assert_eq!(iter2.collect::<Vec<_>>(), vec![2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(iter.clone().as_dyn_iterator().collect::<Vec<_>>(), vec![2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(iter.collect::<Vec<_>>(), vec![2, 3, 4, 5, 6, 7, 8, 9]);
    }
}
