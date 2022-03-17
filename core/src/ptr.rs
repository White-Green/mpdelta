use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::Arc;

pub struct StaticPointerOwned<T>(Arc<T>);

pub struct StaticPointer<T>(Arc<T>);

impl<T> StaticPointerOwned<T> {
    pub fn new(value: T) -> Self {
        StaticPointerOwned(Arc::new(value))
    }

    pub fn reference(this: &Self) -> StaticPointer<T> {
        StaticPointer(Arc::clone(&this.0))
    }
}

impl<T> StaticPointer<T> {
    pub fn reference(this: &Self) -> StaticPointer<T> {
        StaticPointer(Arc::clone(&this.0))
    }
}

impl<T: Debug> Debug for StaticPointerOwned<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(&format!("StaticPointerOwned: {:p}", self.0))
            .field(&*self.0)
            .finish()
    }
}

impl<T: Debug> Debug for StaticPointer<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "StaticPointer: {:p}", self.0)
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

impl<T> Deref for StaticPointerOwned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> Deref for StaticPointer<T> {
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

impl<T: PartialEq> PartialEq for StaticPointer<T> {
    fn eq(&self, other: &Self) -> bool {
        <Arc<T> as PartialEq>::eq(&self.0, &other.0)
    }
}

impl<T: Eq> Eq for StaticPointerOwned<T> {}

impl<T: Eq> Eq for StaticPointer<T> {}

impl<T: PartialEq> PartialEq<StaticPointer<T>> for StaticPointerOwned<T> {
    fn eq(&self, other: &StaticPointer<T>) -> bool {
        <Arc<T> as PartialEq>::eq(&self.0, &other.0)
    }
}

impl<T: PartialEq> PartialEq<StaticPointerOwned<T>> for StaticPointer<T> {
    fn eq(&self, other: &StaticPointerOwned<T>) -> bool {
        <Arc<T> as PartialEq>::eq(&self.0, &other.0)
    }
}

impl<T> Hash for StaticPointerOwned<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state)
    }
}

impl<T> Hash for StaticPointer<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state)
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
        let ptr_format = format!("{:?}", ptr);
        assert!(regex.is_match(&ptr_format));
        let captures = regex.captures(&ptr_format).unwrap();
        assert_eq!(captures.len(), 2);
        let address = captures.get(1).unwrap().as_str();
        assert_eq!(
            format!("{:?}", owned),
            format!("StaticPointerOwned: {}(42)", address)
        );

        #[derive(Debug)]
        struct TestStruct {
            #[allow(unused)]
            value: i32,
        }
        let owned = StaticPointerOwned::new(TestStruct { value: 42 });
        let ptr = StaticPointerOwned::reference(&owned);
        let ptr_format = format!("{:?}", ptr);
        assert!(regex.is_match(&ptr_format));
        let captures = regex.captures(&ptr_format).unwrap();
        assert_eq!(captures.len(), 2);
        let address = captures.get(1).unwrap().as_str();
        assert_eq!(
            format!("{:?}", owned),
            format!(
                "StaticPointerOwned: {}(TestStruct {{ value: 42 }})",
                address
            )
        );

        let ptr_format = format!("{:#?}", ptr);
        assert!(regex.is_match(&ptr_format));
        let captures = regex.captures(&ptr_format).unwrap();
        assert_eq!(captures.len(), 2);
        let address = captures.get(1).unwrap().as_str();
        assert_eq!(
            format!("{:#?}", owned),
            format!(
                "StaticPointerOwned: {}(\n    TestStruct {{\n        value: 42,\n    }},\n)",
                address
            )
        );

        #[derive(Debug)]
        struct TestStructTuple(i32);
        let owned = StaticPointerOwned::new(TestStructTuple(42));
        let ptr = StaticPointerOwned::reference(&owned);
        let ptr_format = format!("{:?}", ptr);
        assert!(regex.is_match(&ptr_format));
        let captures = regex.captures(&ptr_format).unwrap();
        assert_eq!(captures.len(), 2);
        let address = captures.get(1).unwrap().as_str();
        assert_eq!(
            format!("{:?}", owned),
            format!("StaticPointerOwned: {}(TestStructTuple(42))", address)
        );

        let ptr_format = format!("{:#?}", ptr);
        assert!(regex.is_match(&ptr_format));
        let captures = regex.captures(&ptr_format).unwrap();
        assert_eq!(captures.len(), 2);
        let address = captures.get(1).unwrap().as_str();
        assert_eq!(
            format!("{:#?}", owned),
            format!(
                "StaticPointerOwned: {}(\n    TestStructTuple(\n        42,\n    ),\n)",
                address
            )
        );
    }
}
