use std::marker::PhantomData;

pub struct MappedSlice<'a, S, T> {
    slice: &'a [S],
    phantom: PhantomData<T>,
}

pub struct MappedSliceMut<'a, S, T, U> {
    slice: &'a mut [S],
    phantom: PhantomData<(T, U)>,
}

impl<'a, S, T> MappedSlice<'a, S, T> {
    pub fn new(slice: &[S]) -> MappedSlice<'_, S, T> {
        MappedSlice { slice, phantom: Default::default() }
    }

    pub fn get(&self, index: usize) -> Option<T>
    where
        for<'b> &'b S: Into<T>,
    {
        self.slice.get(index).map(Into::into)
    }
}

impl<'a, S, T, U> MappedSliceMut<'a, S, T, U> {
    pub fn new(slice: &mut [S]) -> MappedSliceMut<'_, S, T, U> {
        MappedSliceMut { slice, phantom: Default::default() }
    }

    pub fn get(&self, index: usize) -> Option<T>
    where
        for<'b> &'b S: Into<T>,
    {
        self.slice.get(index).map(Into::into)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<U>
    where
        for<'b> &'b mut S: Into<U>,
    {
        self.slice.get_mut(index).map(Into::into)
    }
}
