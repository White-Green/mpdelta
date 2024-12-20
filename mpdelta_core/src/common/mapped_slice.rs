use crate::common::general_lifetime::AsGeneralLifetime;
use std::marker::PhantomData;

pub struct MappedSlice<'a, S, T> {
    slice: &'a [S],
    phantom: PhantomData<T>,
}

pub struct MappedSliceMut<'a, S, T, U> {
    slice: &'a mut [S],
    phantom: PhantomData<(T, U)>,
}

impl<S, T> MappedSlice<'_, S, T> {
    pub fn new(slice: &[S]) -> MappedSlice<'_, S, T> {
        MappedSlice { slice, phantom: Default::default() }
    }

    pub fn get<'b>(&'b self, index: usize) -> Option<<T as AsGeneralLifetime<'b>>::GeneralLifetimeType>
    where
        T: AsGeneralLifetime<'b>,
        &'b S: Into<<T as AsGeneralLifetime<'b>>::GeneralLifetimeType>,
    {
        self.slice.get(index).map(Into::into)
    }
}

impl<S, T, U> MappedSliceMut<'_, S, T, U> {
    pub fn new(slice: &mut [S]) -> MappedSliceMut<'_, S, T, U> {
        MappedSliceMut { slice, phantom: Default::default() }
    }

    pub fn get<'b>(&'b self, index: usize) -> Option<<T as AsGeneralLifetime<'b>>::GeneralLifetimeType>
    where
        T: AsGeneralLifetime<'b>,
        &'b S: Into<<T as AsGeneralLifetime<'b>>::GeneralLifetimeType>,
    {
        self.slice.get(index).map(Into::into)
    }

    pub fn get_mut<'b>(&'b mut self, index: usize) -> Option<<U as AsGeneralLifetime<'b>>::GeneralLifetimeType>
    where
        U: AsGeneralLifetime<'b>,
        &'b mut S: Into<<U as AsGeneralLifetime<'b>>::GeneralLifetimeType>,
    {
        self.slice.get_mut(index).map(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(unused)]
    fn test_mapped_slice<T>(slice: MappedSlice<'_, T, &T>) {
        slice.get(0);
    }

    #[allow(unused)]
    fn test_mapped_slice_ref<T>(slice: &MappedSlice<'_, T, &T>) {
        slice.get(0);
    }

    #[allow(unused)]
    fn test_mapped_slice_mut<T>(mut slice: MappedSliceMut<'_, T, &T, &mut T>) {
        slice.get(0);
        slice.get_mut(0);
    }

    #[allow(unused)]
    fn test_mapped_slice_ref_mut<T>(slice: &mut MappedSliceMut<'_, T, &T, &mut T>) {
        slice.get(0);
        slice.get_mut(0);
    }
}
