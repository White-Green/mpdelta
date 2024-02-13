use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::Deref;
use std::ptr;
use std::sync::{Arc, Weak};

pub struct StaticPointerOwned<T: ?Sized>(Arc<T>, StaticPointer<T>);

pub struct StaticPointer<T: ?Sized>(Weak<T>);

pub struct StaticPointerStrongRef<'a, T: ?Sized>(Arc<T>, PhantomData<&'a ()>);

#[derive(Debug)]
pub enum StaticPointerCow<T: ?Sized> {
    Owned(StaticPointerOwned<T>),
    Reference(StaticPointer<T>),
}

impl<T> StaticPointerOwned<T> {
    pub fn new(value: T) -> Self {
        let inner = Arc::new(value);
        let weak = Arc::downgrade(&inner);
        StaticPointerOwned(inner, StaticPointer(weak))
    }

    pub fn map<U: ?Sized>(self, map: fn(Arc<T>) -> Arc<U>, map_weak: fn(Weak<T>) -> Weak<U>) -> StaticPointerOwned<U> {
        StaticPointerOwned(map(self.0), self.1.map(map_weak))
    }
}

impl<T: ?Sized> StaticPointerOwned<T> {
    pub fn reference(this: &Self) -> &StaticPointer<T> {
        &this.1
    }
}

impl<T: Clone> Clone for StaticPointerOwned<T> {
    fn clone(&self) -> Self {
        let value = T::clone(self);
        StaticPointerOwned::new(value)
    }
}

impl<T: ?Sized> AsRef<StaticPointer<T>> for StaticPointerOwned<T> {
    fn as_ref(&self) -> &StaticPointer<T> {
        &self.1
    }
}

impl<T> Default for StaticPointer<T> {
    fn default() -> Self {
        StaticPointer::new()
    }
}

impl<T> StaticPointer<T> {
    pub fn new() -> StaticPointer<T> {
        StaticPointer(Weak::new())
    }
}

impl<T: ?Sized> StaticPointer<T> {
    pub fn reference(&self) -> StaticPointer<T> {
        StaticPointer(Weak::clone(&self.0))
    }

    pub fn map<U: ?Sized>(self, map: fn(Weak<T>) -> Weak<U>) -> StaticPointer<U> {
        StaticPointer(map(self.0))
    }

    pub fn upgrade(&self) -> Option<StaticPointerStrongRef<'_, T>> {
        self.0.upgrade().map(|strong_ref| StaticPointerStrongRef(strong_ref, PhantomData))
    }

    pub fn may_upgrade(&self) -> bool {
        self.0.strong_count() > 0
    }
}

impl<'a, T: ?Sized> StaticPointerStrongRef<'a, tokio::sync::RwLock<T>> {
    pub fn read_owned(this: Self) -> impl Future<Output = tokio::sync::OwnedRwLockReadGuard<T>> {
        this.0.read_owned()
    }

    pub fn write_owned(this: Self) -> impl Future<Output = tokio::sync::OwnedRwLockWriteGuard<T>> {
        this.0.write_owned()
    }
}

impl<T: ?Sized + Debug> Debug for StaticPointerOwned<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(&format!("StaticPointerOwned: {:p}", self.0)).field(&&self.0).finish()
    }
}

impl<T: ?Sized> Debug for StaticPointer<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "StaticPointer: {:p}", self.0.as_ptr())
    }
}

impl<'a, T: ?Sized + Debug> Debug for StaticPointerStrongRef<'a, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(&format!("StaticPointerStrongRef: {:p}", self.0)).field(&&self.0).finish()
    }
}

impl<'a, T: ?Sized> From<&'a StaticPointerOwned<T>> for StaticPointer<T> {
    fn from(ptr: &'a StaticPointerOwned<T>) -> Self {
        StaticPointerOwned::reference(ptr).clone()
    }
}

impl<'a, T: ?Sized> From<&'a StaticPointer<T>> for StaticPointer<T> {
    fn from(ptr: &'a StaticPointer<T>) -> Self {
        StaticPointer::reference(ptr)
    }
}

impl<T: ?Sized> Clone for StaticPointer<T> {
    fn clone(&self) -> Self {
        StaticPointer(Weak::clone(&self.0))
    }
}

impl<T: ?Sized> Deref for StaticPointerOwned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a, T: ?Sized> Deref for StaticPointerStrongRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: ?Sized + PartialEq> PartialEq for StaticPointerOwned<T> {
    fn eq(&self, other: &Self) -> bool {
        <T as PartialEq>::eq(&self.0, &other.0)
    }
}

impl<T: ?Sized> PartialEq for StaticPointer<T> {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}

impl<'a, 'b, T: ?Sized + PartialEq> PartialEq<StaticPointerStrongRef<'b, T>> for StaticPointerStrongRef<'a, T> {
    fn eq(&self, other: &StaticPointerStrongRef<'b, T>) -> bool {
        <T as PartialEq>::eq(&self.0, &other.0)
    }
}

impl<'a, T: ?Sized + PartialEq> PartialEq<StaticPointerStrongRef<'a, T>> for StaticPointerOwned<T> {
    fn eq(&self, other: &StaticPointerStrongRef<'a, T>) -> bool {
        <T as PartialEq>::eq(&self.0, &other.0)
    }
}

impl<'a, T: ?Sized + PartialEq> PartialEq<StaticPointerOwned<T>> for StaticPointerStrongRef<'a, T> {
    fn eq(&self, other: &StaticPointerOwned<T>) -> bool {
        <T as PartialEq>::eq(&self.0, &other.0)
    }
}

impl<T: ?Sized> PartialEq<StaticPointerOwned<T>> for StaticPointer<T> {
    fn eq(&self, other: &StaticPointerOwned<T>) -> bool {
        ptr::eq(Weak::as_ptr(&self.0), Arc::as_ptr(&other.0))
    }
}

impl<T: ?Sized> PartialEq<StaticPointer<T>> for StaticPointerOwned<T> {
    fn eq(&self, other: &StaticPointer<T>) -> bool {
        ptr::eq(Arc::as_ptr(&self.0), Weak::as_ptr(&other.0))
    }
}

impl<'a, T: ?Sized> PartialEq<StaticPointerStrongRef<'a, T>> for StaticPointer<T> {
    fn eq(&self, other: &StaticPointerStrongRef<'a, T>) -> bool {
        ptr::eq(Weak::as_ptr(&self.0), Arc::as_ptr(&other.0))
    }
}

impl<'a, T: ?Sized> PartialEq<StaticPointer<T>> for StaticPointerStrongRef<'a, T> {
    fn eq(&self, other: &StaticPointer<T>) -> bool {
        ptr::eq(Arc::as_ptr(&self.0), Weak::as_ptr(&other.0))
    }
}

impl<T: ?Sized + Eq> Eq for StaticPointerOwned<T> {}

impl<T: ?Sized> Eq for StaticPointer<T> {}

impl<'a, T: ?Sized + Eq> Eq for StaticPointerStrongRef<'a, T> {}

impl<T: ?Sized + Hash> Hash for StaticPointerOwned<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        T::hash(&self.0, state)
    }
}

impl<T: ?Sized> Hash for StaticPointer<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Weak::as_ptr(&self.0).hash(state)
    }
}

impl<T: ?Sized> StaticPointerCow<T> {
    pub fn ptr(&self) -> &StaticPointer<T> {
        self.as_ref()
    }
}

impl<T: ?Sized> From<StaticPointerOwned<T>> for StaticPointerCow<T> {
    fn from(value: StaticPointerOwned<T>) -> Self {
        StaticPointerCow::Owned(value)
    }
}

impl<'a, T: ?Sized> From<&'a StaticPointerOwned<T>> for StaticPointerCow<T> {
    fn from(value: &'a StaticPointerOwned<T>) -> Self {
        StaticPointerCow::Reference(StaticPointerOwned::reference(value).clone())
    }
}

impl<T: ?Sized> From<StaticPointer<T>> for StaticPointerCow<T> {
    fn from(value: StaticPointer<T>) -> Self {
        StaticPointerCow::Reference(value)
    }
}

impl<T: ?Sized> Clone for StaticPointerCow<T> {
    fn clone(&self) -> Self {
        let ptr = match self {
            StaticPointerCow::Owned(owned) => StaticPointerOwned::reference(owned),
            StaticPointerCow::Reference(ptr) => ptr,
        };
        StaticPointerCow::Reference(ptr.clone())
    }
}

impl<T: ?Sized> AsRef<StaticPointer<T>> for StaticPointerCow<T> {
    fn as_ref(&self) -> &StaticPointer<T> {
        match self {
            StaticPointerCow::Owned(value) => StaticPointerOwned::reference(value),
            StaticPointerCow::Reference(value) => value,
        }
    }
}

impl<T: ?Sized> AsRef<StaticPointer<T>> for StaticPointer<T> {
    fn as_ref(&self) -> &StaticPointer<T> {
        self
    }
}

impl<T: ?Sized> Deref for StaticPointerCow<T> {
    type Target = StaticPointer<T>;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use crate::ptr::StaticPointerOwned;
    use regex::Regex;

    #[test]
    fn debug_format() {
        let regex = Regex::new("^StaticPointer: (0x[0-9a-f]+)$").unwrap();

        let owned = StaticPointerOwned::new(42);
        let ptr = StaticPointerOwned::reference(&owned).clone();
        let strong_ref = ptr.upgrade().unwrap();
        let ptr_format = format!("{:?}", ptr);
        assert!(regex.is_match(&ptr_format));
        let captures = regex.captures(&ptr_format).unwrap();
        assert_eq!(captures.len(), 2);
        let address = captures.get(1).unwrap().as_str();
        assert_eq!(format!("{:?}", owned), format!("StaticPointerOwned: {}(42)", address));
        assert_eq!(format!("{:?}", strong_ref), format!("StaticPointerStrongRef: {}(42)", address));

        #[derive(Debug)]
        struct TestStruct {
            #[allow(unused)]
            value: i32,
        }
        let owned = StaticPointerOwned::new(TestStruct { value: 42 });
        let ptr = StaticPointerOwned::reference(&owned).clone();
        let strong_ref = ptr.upgrade().unwrap();
        let ptr_format = format!("{:?}", ptr);
        assert!(regex.is_match(&ptr_format));
        let captures = regex.captures(&ptr_format).unwrap();
        assert_eq!(captures.len(), 2);
        let address = captures.get(1).unwrap().as_str();
        assert_eq!(format!("{:?}", owned), format!("StaticPointerOwned: {}(TestStruct {{ value: 42 }})", address));
        assert_eq!(format!("{:?}", strong_ref), format!("StaticPointerStrongRef: {}(TestStruct {{ value: 42 }})", address));

        let ptr_format = format!("{:#?}", ptr);
        assert!(regex.is_match(&ptr_format));
        let captures = regex.captures(&ptr_format).unwrap();
        assert_eq!(captures.len(), 2);
        let address = captures.get(1).unwrap().as_str();
        assert_eq!(format!("{:#?}", owned), format!("StaticPointerOwned: {}(\n    TestStruct {{\n        value: 42,\n    }},\n)", address));
        assert_eq!(format!("{:#?}", strong_ref), format!("StaticPointerStrongRef: {}(\n    TestStruct {{\n        value: 42,\n    }},\n)", address));

        #[derive(Debug)]
        struct TestStructTuple(i32);
        let owned = StaticPointerOwned::new(TestStructTuple(42));
        let ptr = StaticPointerOwned::reference(&owned).clone();
        let strong_ref = ptr.upgrade().unwrap();
        let ptr_format = format!("{:?}", ptr);
        assert!(regex.is_match(&ptr_format));
        let captures = regex.captures(&ptr_format).unwrap();
        assert_eq!(captures.len(), 2);
        let address = captures.get(1).unwrap().as_str();
        assert_eq!(format!("{:?}", owned), format!("StaticPointerOwned: {}(TestStructTuple(42))", address));
        assert_eq!(format!("{:?}", strong_ref), format!("StaticPointerStrongRef: {}(TestStructTuple(42))", address));

        let ptr_format = format!("{:#?}", ptr);
        assert!(regex.is_match(&ptr_format));
        let captures = regex.captures(&ptr_format).unwrap();
        assert_eq!(captures.len(), 2);
        let address = captures.get(1).unwrap().as_str();
        assert_eq!(format!("{:#?}", owned), format!("StaticPointerOwned: {}(\n    TestStructTuple(\n        42,\n    ),\n)", address));
        assert_eq!(format!("{:#?}", strong_ref), format!("StaticPointerStrongRef: {}(\n    TestStructTuple(\n        42,\n    ),\n)", address));
    }

    #[test]
    fn eq() {
        let owned1 = StaticPointerOwned::new(());
        let owned2 = StaticPointerOwned::new(());
        let weak11 = StaticPointerOwned::reference(&owned1).clone();
        let weak12 = StaticPointerOwned::reference(&owned1).clone();
        let weak21 = StaticPointerOwned::reference(&owned2).clone();
        let weak22 = weak21.clone();
        let strong11 = weak11.upgrade().unwrap();
        let strong12 = weak12.upgrade().unwrap();
        let strong21 = weak21.upgrade().unwrap();
        let strong22 = weak22.upgrade().unwrap();
        assert_eq!(owned1, weak11);
        assert_eq!(owned1, weak12);
        assert_eq!(owned2, weak21);
        assert_eq!(owned2, weak22);
        assert_eq!(strong11, weak11);
        assert_eq!(strong11, weak12);
        assert_eq!(strong12, weak11);
        assert_eq!(strong12, weak12);
        assert_eq!(strong21, weak21);
        assert_eq!(strong21, weak22);
        assert_eq!(strong22, weak21);
        assert_eq!(strong22, weak22);
        assert_eq!(weak11, weak12);
        assert_eq!(weak21, weak22);

        assert_ne!(owned1, weak21);
        assert_ne!(owned1, weak22);
        assert_ne!(owned2, weak11);
        assert_ne!(owned2, weak12);
        assert_ne!(strong21, weak11);
        assert_ne!(strong21, weak12);
        assert_ne!(strong22, weak11);
        assert_ne!(strong22, weak12);
        assert_ne!(strong11, weak21);
        assert_ne!(strong11, weak22);
        assert_ne!(strong12, weak21);
        assert_ne!(strong12, weak22);
        assert_ne!(weak11, weak22);
        assert_ne!(weak12, weak22);
    }

    #[test]
    fn may_can_upgrade() {
        let owned = StaticPointerOwned::new(());
        let ptr = StaticPointerOwned::reference(&owned).clone();
        assert!(ptr.may_upgrade());
        drop(owned);
        assert!(!ptr.may_upgrade());
    }
}
