use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::{Arc, Weak};

pub struct StaticPointerOwned<T>(Arc<T>);

pub struct StaticPointer<T>(Weak<T>);

pub struct StaticPointerStrongRef<'a, T>(Arc<T>, PhantomData<&'a ()>);

impl<T> StaticPointerOwned<T> {
    pub fn new(value: T) -> Self {
        StaticPointerOwned(Arc::new(value))
    }

    pub fn reference(this: &Self) -> StaticPointer<T> {
        StaticPointer(Arc::downgrade(&this.0))
    }
}

impl<T> StaticPointer<T> {
    pub fn reference(&self) -> StaticPointer<T> {
        StaticPointer(Weak::clone(&self.0))
    }

    pub fn upgrade(&self) -> Option<StaticPointerStrongRef<'_, T>> {
        self.0.upgrade().map(|strong_ref| StaticPointerStrongRef(strong_ref, PhantomData::default()))
    }
}

impl<T: Debug> Debug for StaticPointerOwned<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(&format!("StaticPointerOwned: {:p}", self.0)).field(&*self.0).finish()
    }
}

impl<T: Debug> Debug for StaticPointer<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "StaticPointer: {:p}", self.0.as_ptr())
    }
}

impl<'a, T: Debug> Debug for StaticPointerStrongRef<'a, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(&format!("StaticPointerStrongRef: {:p}", self.0)).field(&*self.0).finish()
    }
}

impl<'a, T> From<&'a StaticPointerOwned<T>> for StaticPointer<T> {
    fn from(ptr: &'a StaticPointerOwned<T>) -> Self {
        StaticPointerOwned::reference(ptr)
    }
}

impl<'a, T> From<&'a StaticPointer<T>> for StaticPointer<T> {
    fn from(ptr: &'a StaticPointer<T>) -> Self {
        StaticPointer::reference(ptr)
    }
}

impl<T> Clone for StaticPointer<T> {
    fn clone(&self) -> Self {
        StaticPointer(Weak::clone(&self.0))
    }
}

impl<T> Deref for StaticPointerOwned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a, T> Deref for StaticPointerStrongRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: PartialEq> PartialEq for StaticPointerOwned<T> {
    fn eq(&self, other: &Self) -> bool {
        <Arc<T> as PartialEq>::eq(&self.0, &other.0)
    }
}

impl<T> PartialEq for StaticPointer<T> {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}

impl<'a, 'b, T: PartialEq> PartialEq<StaticPointerStrongRef<'b, T>> for StaticPointerStrongRef<'a, T> {
    fn eq(&self, other: &StaticPointerStrongRef<'b, T>) -> bool {
        <Arc<T> as PartialEq>::eq(&self.0, &other.0)
    }
}

impl<'a, T: PartialEq> PartialEq<StaticPointerStrongRef<'a, T>> for StaticPointerOwned<T> {
    fn eq(&self, other: &StaticPointerStrongRef<'a, T>) -> bool {
        <Arc<T> as PartialEq>::eq(&self.0, &other.0)
    }
}

impl<'a, T: PartialEq> PartialEq<StaticPointerOwned<T>> for StaticPointerStrongRef<'a, T> {
    fn eq(&self, other: &StaticPointerOwned<T>) -> bool {
        <Arc<T> as PartialEq>::eq(&self.0, &other.0)
    }
}

impl<T: Eq> Eq for StaticPointerOwned<T> {}

impl<T> Eq for StaticPointer<T> {}

impl<'a, T: Eq> Eq for StaticPointerStrongRef<'a, T> {}

impl<T: Hash> Hash for StaticPointerOwned<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        T::hash(&self.0, state)
    }
}

impl<T> Hash for StaticPointer<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Weak::as_ptr(&self.0).hash(state)
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
        let ptr = StaticPointerOwned::reference(&owned);
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
        let ptr = StaticPointerOwned::reference(&owned);
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
        let ptr = StaticPointerOwned::reference(&owned);
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
}
