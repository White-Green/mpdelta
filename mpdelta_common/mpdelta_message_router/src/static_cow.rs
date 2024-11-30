use std::borrow::Cow;

pub trait StaticCow<T> {
    type Cloned<'a>: StaticCow<T>
    where
        Self: 'a;
    fn clone(&self) -> Self::Cloned<'_>;
    fn with_ref<R>(&self, f: impl FnOnce(&T) -> R) -> R;
    fn into_owned(self) -> T;
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Reference<'a, T>(pub &'a T);

impl<T: Clone> StaticCow<T> for Reference<'_, T> {
    type Cloned<'b>
        = Reference<'b, T>
    where
        Self: 'b;

    fn clone(&self) -> Self::Cloned<'_> {
        Reference(self.0)
    }

    fn with_ref<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(self.0)
    }

    fn into_owned(self) -> T {
        self.0.clone()
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Owned<T>(pub T);

impl<T: Clone> StaticCow<T> for Owned<T> {
    type Cloned<'a>
        = Reference<'a, T>
    where
        Self: 'a;

    fn clone(&self) -> Self::Cloned<'_> {
        Reference(&self.0)
    }

    fn with_ref<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.0)
    }

    fn into_owned(self) -> T {
        self.0
    }
}

impl<T: Clone> StaticCow<T> for Cow<'_, T> {
    type Cloned<'b>
        = Reference<'b, T>
    where
        Self: 'b;

    fn clone(&self) -> Self::Cloned<'_> {
        Reference(self)
    }

    fn with_ref<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(self)
    }

    fn into_owned(self) -> T {
        self.into_owned()
    }
}
