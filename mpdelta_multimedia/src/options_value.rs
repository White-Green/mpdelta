use std::borrow::Cow;
use std::ops::{Bound, RangeBounds};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueWithDefault<T> {
    Default,
    Value(T),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueTypeI64 {
    Any,
    Range(Bound<i64>, Bound<i64>),
    Candidates(Arc<[i64]>),
}

impl ValueTypeI64 {
    pub fn validate(&self, value: i64) -> bool {
        match self {
            ValueTypeI64::Any => true,
            ValueTypeI64::Range(min, max) => (min.as_ref(), max.as_ref()).contains(&value),
            ValueTypeI64::Candidates(candidates) => candidates.contains(&value),
        }
    }
}

impl<R> From<R> for ValueTypeI64
where
    R: RangeBounds<i64>,
{
    fn from(value: R) -> Self {
        ValueTypeI64::Range(value.start_bound().cloned(), value.end_bound().cloned())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueTypeF64 {
    Any,
    Range(Bound<f64>, Bound<f64>),
    Candidates(Arc<[f64]>),
}

impl ValueTypeF64 {
    pub fn validate(&self, value: f64) -> bool {
        match self {
            ValueTypeF64::Any => true,
            ValueTypeF64::Range(min, max) => (min.as_ref(), max.as_ref()).contains(&value),
            ValueTypeF64::Candidates(candidates) => candidates.contains(&value),
        }
    }
}

impl<R> From<R> for ValueTypeF64
where
    R: RangeBounds<f64>,
{
    fn from(value: R) -> Self {
        ValueTypeF64::Range(value.start_bound().cloned(), value.end_bound().cloned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueTypeString {
    Any,
    Candidates(Arc<[Cow<'static, str>]>),
}

impl ValueTypeString {
    pub fn validate(&self, value: &str) -> bool {
        match self {
            ValueTypeString::Any => true,
            ValueTypeString::Candidates(candidates) => candidates.iter().any(|c| c == value),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OptionValue {
    Bool { value: ValueWithDefault<bool> },
    Int { value: ValueWithDefault<i64>, ty: ValueTypeI64 },
    Float { value: ValueWithDefault<f64>, ty: ValueTypeF64 },
    String { value: ValueWithDefault<String>, ty: ValueTypeString },
}

#[derive(Debug, PartialEq)]
pub enum OptionValuesRefMut<'a> {
    Bool { value: &'a mut ValueWithDefault<bool> },
    Int { value: &'a mut ValueWithDefault<i64>, ty: &'a ValueTypeI64 },
    Float { value: &'a mut ValueWithDefault<f64>, ty: &'a ValueTypeF64 },
    String { value: &'a mut ValueWithDefault<String>, ty: &'a ValueTypeString },
}

impl OptionValue {
    pub fn as_ref(&mut self) -> OptionValuesRefMut {
        match self {
            OptionValue::Bool { value } => OptionValuesRefMut::Bool { value },
            OptionValue::Int { value, ty } => OptionValuesRefMut::Int { value, ty },
            OptionValue::Float { value, ty } => OptionValuesRefMut::Float { value, ty },
            OptionValue::String { value, ty } => OptionValuesRefMut::String { value, ty },
        }
    }

    pub fn validate(&self) -> bool {
        match self {
            OptionValue::Bool { .. } => true,
            OptionValue::Int { value: ValueWithDefault::Default, .. } => true,
            OptionValue::Int { value: ValueWithDefault::Value(value), ty } => ty.validate(*value),
            OptionValue::Float { value: ValueWithDefault::Default, .. } => true,
            OptionValue::Float { value: ValueWithDefault::Value(value), ty } => ty.validate(*value),
            OptionValue::String { value: ValueWithDefault::Default, .. } => true,
            OptionValue::String { value: ValueWithDefault::Value(value), ty } => ty.validate(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_i64() {
        assert!(ValueTypeI64::Any.validate(0));
        assert!(ValueTypeI64::from(..).validate(0));
        assert!(ValueTypeI64::from(0..).validate(0));
        assert!(!ValueTypeI64::from(0..).validate(-1));
        assert!(ValueTypeI64::from(..0).validate(-1));
        assert!(!ValueTypeI64::from(..0).validate(0));
        assert!(ValueTypeI64::Candidates(vec![0, 1, 2].into()).validate(0));
        assert!(!ValueTypeI64::Candidates(vec![0, 1, 2].into()).validate(3));
    }

    #[test]
    fn test_validate_f64() {
        assert!(ValueTypeF64::Any.validate(0.));
        assert!(ValueTypeF64::from(..).validate(0.));
        assert!(ValueTypeF64::from(0.0..).validate(0.));
        assert!(!ValueTypeF64::from(0.0..).validate(-1.));
        assert!(ValueTypeF64::from(..0.0).validate(-1.));
        assert!(!ValueTypeF64::from(..0.0).validate(0.));
        assert!(ValueTypeF64::Candidates(vec![0., 1., 2.].into()).validate(0.));
        assert!(!ValueTypeF64::Candidates(vec![0., 1., 2.].into()).validate(3.));
    }

    #[test]
    fn test_validate_string() {
        assert!(ValueTypeString::Any.validate(""));
        assert!(ValueTypeString::Candidates(vec!["".into(), "a".into(), "b".into()].into()).validate(""));
        assert!(!ValueTypeString::Candidates(vec!["".into(), "a".into(), "b".into()].into()).validate("c"));
    }
}
