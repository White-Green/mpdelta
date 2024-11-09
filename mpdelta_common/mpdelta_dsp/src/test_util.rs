use crate::WindowFunction;
use std::ops::{Add, Mul};

#[derive(Debug, Default, Clone, PartialEq)]
pub enum FormalExpression {
    #[default]
    Zero,
    Sum(Vec<FormalExpression>),
    Product(Vec<FormalExpression>),
    Value(usize),
    Window(usize),
}

impl FormalExpression {
    pub fn value(value: usize) -> Self {
        FormalExpression::Value(value)
    }
}

impl WindowFunction for FormalExpression {
    fn window(length: usize, _target_sum: f64) -> Box<[Self]> {
        (0..length).map(FormalExpression::Window).collect()
    }
}

impl Add for FormalExpression {
    type Output = FormalExpression;

    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (FormalExpression::Zero, x) | (x, FormalExpression::Zero) => x,
            (FormalExpression::Sum(mut x), FormalExpression::Sum(y)) => {
                x.extend(y);
                FormalExpression::Sum(x)
            }
            (FormalExpression::Sum(mut x), y) => {
                x.push(y);
                FormalExpression::Sum(x)
            }
            (x, FormalExpression::Sum(mut y)) => {
                y.insert(0, x);
                FormalExpression::Sum(y)
            }
            (x, y) => FormalExpression::Sum(vec![x, y]),
        }
    }
}

impl Mul for FormalExpression {
    type Output = FormalExpression;

    fn mul(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (FormalExpression::Zero, _) | (_, FormalExpression::Zero) => FormalExpression::Zero,
            (FormalExpression::Product(mut x), FormalExpression::Product(y)) => {
                x.extend(y);
                FormalExpression::Product(x)
            }
            (FormalExpression::Product(mut x), y) => {
                x.push(y);
                FormalExpression::Product(x)
            }
            (x, FormalExpression::Product(mut y)) => {
                y.insert(0, x);
                FormalExpression::Product(y)
            }
            (x, y) => FormalExpression::Product(vec![x, y]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_formal_expression() {
        assert_eq!(*FormalExpression::window(5, 1.), [FormalExpression::Window(0), FormalExpression::Window(1), FormalExpression::Window(2), FormalExpression::Window(3), FormalExpression::Window(4)]);
    }

    #[test]
    fn test_add_formal_expression() {
        assert_eq!(FormalExpression::Value(1) + FormalExpression::Zero, FormalExpression::Value(1));
        assert_eq!(FormalExpression::Value(1) + FormalExpression::Value(2), FormalExpression::Sum(vec![FormalExpression::Value(1), FormalExpression::Value(2)]));
        assert_eq!(
            FormalExpression::Value(1) + FormalExpression::Sum(vec![FormalExpression::Value(2), FormalExpression::Value(3)]),
            FormalExpression::Sum(vec![FormalExpression::Value(1), FormalExpression::Value(2), FormalExpression::Value(3)])
        );
        assert_eq!(
            FormalExpression::Sum(vec![FormalExpression::Value(1), FormalExpression::Value(2)]) + FormalExpression::Value(3),
            FormalExpression::Sum(vec![FormalExpression::Value(1), FormalExpression::Value(2), FormalExpression::Value(3)])
        );
        assert_eq!(
            FormalExpression::Sum(vec![FormalExpression::Value(1), FormalExpression::Value(2)]) + FormalExpression::Sum(vec![FormalExpression::Value(3), FormalExpression::Value(4)]),
            FormalExpression::Sum(vec![FormalExpression::Value(1), FormalExpression::Value(2), FormalExpression::Value(3), FormalExpression::Value(4)])
        );
    }

    #[test]
    fn test_mul_formal_expression() {
        assert_eq!(FormalExpression::Value(1) * FormalExpression::Zero, FormalExpression::Zero);
        assert_eq!(FormalExpression::Value(1) * FormalExpression::Value(2), FormalExpression::Product(vec![FormalExpression::Value(1), FormalExpression::Value(2)]));
        assert_eq!(
            FormalExpression::Value(1) * FormalExpression::Product(vec![FormalExpression::Value(2), FormalExpression::Value(3)]),
            FormalExpression::Product(vec![FormalExpression::Value(1), FormalExpression::Value(2), FormalExpression::Value(3)])
        );
        assert_eq!(
            FormalExpression::Product(vec![FormalExpression::Value(1), FormalExpression::Value(2)]) * FormalExpression::Value(3),
            FormalExpression::Product(vec![FormalExpression::Value(1), FormalExpression::Value(2), FormalExpression::Value(3)])
        );
        assert_eq!(
            FormalExpression::Product(vec![FormalExpression::Value(1), FormalExpression::Value(2)]) * FormalExpression::Product(vec![FormalExpression::Value(3), FormalExpression::Value(4)]),
            FormalExpression::Product(vec![FormalExpression::Value(1), FormalExpression::Value(2), FormalExpression::Value(3), FormalExpression::Value(4)])
        );
    }
}
