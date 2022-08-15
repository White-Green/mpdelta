pub trait AbstractSlice<T> {
    fn get(&self, index: usize) -> Option<&T>;
    fn iter(&self) -> Box<dyn Iterator<Item = &T> + '_>;
}

pub trait AbstractSliceMut<T>: AbstractSlice<T> {
    fn get_mut(&mut self, index: usize) -> Option<&mut T>;
    fn iter_mut(&mut self) -> Box<dyn Iterator<Item = &mut T> + '_>;
}

impl<T> AbstractSlice<T> for [T] {
    fn get(&self, index: usize) -> Option<&T> {
        self.get(index)
    }

    fn iter(&self) -> Box<dyn Iterator<Item = &T> + '_> {
        Box::new(self.iter())
    }
}

impl<T> AbstractSliceMut<T> for [T] {
    fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.get_mut(index)
    }

    fn iter_mut(&mut self) -> Box<dyn Iterator<Item = &mut T> + '_> {
        Box::new(self.iter_mut())
    }
}

impl<'a, T> AbstractSlice<T> for [&'a T] {
    fn get(&self, index: usize) -> Option<&T> {
        self.get(index).map(|v| *v)
    }

    fn iter(&self) -> Box<dyn Iterator<Item = &T> + '_> {
        Box::new(self.iter().map(|v| *v))
    }
}

impl<'a, T> AbstractSlice<T> for [&'a mut T] {
    fn get(&self, index: usize) -> Option<&T> {
        self.get(index).map(|v| &**v)
    }

    fn iter(&self) -> Box<dyn Iterator<Item = &T> + '_> {
        Box::new(self.iter().map(|v| &**v))
    }
}

impl<'a, T> AbstractSliceMut<T> for [&'a mut T] {
    fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.get_mut(index).map(|v| &mut **v)
    }

    fn iter_mut(&mut self) -> Box<dyn Iterator<Item = &mut T> + '_> {
        Box::new(self.iter_mut().map(|v| &mut **v))
    }
}

impl<'a, T, S: ?Sized + AbstractSlice<T>> AbstractSlice<T> for &'a S {
    fn get(&self, index: usize) -> Option<&T> {
        <S as AbstractSlice<T>>::get(self, index)
    }

    fn iter(&self) -> Box<dyn Iterator<Item = &T> + '_> {
        <S as AbstractSlice<T>>::iter(self)
    }
}

impl<'a, T, S: ?Sized + AbstractSlice<T>> AbstractSlice<T> for &'a mut S {
    fn get(&self, index: usize) -> Option<&T> {
        <S as AbstractSlice<T>>::get(self, index)
    }

    fn iter(&self) -> Box<dyn Iterator<Item = &T> + '_> {
        <S as AbstractSlice<T>>::iter(self)
    }
}

impl<'a, T, S: ?Sized + AbstractSliceMut<T>> AbstractSliceMut<T> for &'a mut S {
    fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        <S as AbstractSliceMut<T>>::get_mut(self, index)
    }

    fn iter_mut(&mut self) -> Box<dyn Iterator<Item = &mut T> + '_> {
        <S as AbstractSliceMut<T>>::iter_mut(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abstract_slice() {
        let mut array = vec![1, 2, 3, 4, 5];
        assert_eq!(<[i32] as AbstractSlice<i32>>::get(array.as_slice(), 0), Some(&1));
        assert_eq!(<[i32] as AbstractSlice<i32>>::iter(array.as_slice()).collect::<Vec<_>>(), vec![&1, &2, &3, &4, &5]);
        assert_eq!(<[i32] as AbstractSliceMut<i32>>::get_mut(array.as_mut_slice(), 0), Some(&mut 1));
        assert_eq!(<[i32] as AbstractSliceMut<i32>>::iter_mut(array.as_mut_slice()).collect::<Vec<_>>(), vec![&mut 1, &mut 2, &mut 3, &mut 4, &mut 5]);

        let mut array = vec![1, 2, 3, 4, 5];
        assert_eq!(<&[i32] as AbstractSlice<i32>>::get(&array.as_slice(), 0), Some(&1));
        assert_eq!(<&[i32] as AbstractSlice<i32>>::iter(&array.as_slice()).collect::<Vec<_>>(), vec![&1, &2, &3, &4, &5]);
        assert_eq!(<&mut [i32] as AbstractSlice<i32>>::get(&array.as_mut_slice(), 0), Some(&1));
        assert_eq!(<&mut [i32] as AbstractSlice<i32>>::iter(&array.as_mut_slice()).collect::<Vec<_>>(), vec![&1, &2, &3, &4, &5]);
        assert_eq!(<&mut [i32] as AbstractSliceMut<i32>>::get_mut(&mut array.as_mut_slice(), 0), Some(&mut 1));
        assert_eq!(<&mut [i32] as AbstractSliceMut<i32>>::iter_mut(&mut array.as_mut_slice()).collect::<Vec<_>>(), vec![&mut 1, &mut 2, &mut 3, &mut 4, &mut 5]);

        let array = vec![1, 2, 3, 4, 5];
        let array = array.iter().collect::<Vec<_>>();
        assert_eq!(<[&i32] as AbstractSlice<i32>>::get(array.as_slice(), 0), Some(&1));
        assert_eq!(<[&i32] as AbstractSlice<i32>>::iter(array.as_slice()).collect::<Vec<_>>(), vec![&1, &2, &3, &4, &5]);

        let mut array = vec![1, 2, 3, 4, 5];
        let mut array = array.iter_mut().collect::<Vec<_>>();
        assert_eq!(<[&mut i32] as AbstractSlice<i32>>::get(array.as_slice(), 0), Some(&1));
        assert_eq!(<[&mut i32] as AbstractSlice<i32>>::iter(array.as_slice()).collect::<Vec<_>>(), vec![&1, &2, &3, &4, &5]);
        assert_eq!(<[&mut i32] as AbstractSliceMut<i32>>::get_mut(array.as_mut_slice(), 0), Some(&mut 1));
        assert_eq!(<[&mut i32] as AbstractSliceMut<i32>>::iter_mut(array.as_mut_slice()).collect::<Vec<_>>(), vec![&mut 1, &mut 2, &mut 3, &mut 4, &mut 5]);
    }
}
