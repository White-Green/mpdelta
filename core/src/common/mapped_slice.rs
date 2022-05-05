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

impl<'a, S, T> MappedSlice<'a, S, T> {
    pub fn new(slice: &[S]) -> MappedSlice<'_, S, T> {
        MappedSlice { slice, phantom: Default::default() }
    }

    pub fn get(&self, index: usize) -> Option<<T as AsGeneralLifetime<'_>>::GeneralLifetimeType>
    where
        for<'b> &'b S: Into<T>,
        for<'b> T: AsGeneralLifetime<'b>,
    {
        self.slice.get(index).map(Into::into).map(AsGeneralLifetime::into_general_lifetime)
    }
}

impl<'a, S, T, U> MappedSliceMut<'a, S, T, U> {
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
