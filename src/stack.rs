use std::{collections::VecDeque, str::FromStr};

use bigdecimal::{BigDecimal, ParseBigDecimalError, Pow, ToPrimitive, Zero};
use thiserror::Error;

use crate::state::State;

pub struct Stack {
    s: VecDeque<BigDecimal>,
    precision: u64,
}

#[derive(Error, Debug, PartialEq)]
pub enum StackError {
    #[error("operation requires {0} elements")]
    MissingValue(usize),
    #[error("{0}")]
    InvalidArgument(String),
}

#[derive(Debug, Clone)]
pub enum Op {
    Push(BigDecimal),
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Sqrt,
    Pow,
    Duplicate,
    Pop,
    Precision,
    Rotate,
}

impl Stack {
    #[cfg(test)]
    pub fn new() -> Stack {
        Stack {
            s: VecDeque::new(),
            precision: 12,
        }
    }

    pub fn from(values: Vec<BigDecimal>) -> Stack {
        Stack {
            s: values.into(),
            precision: 12,
        }
    }

    pub fn apply(&mut self, op: Op) -> Result<(), StackError> {
        match op {
            Op::Push(v) => {
                self.s.push_front(v);
            }
            Op::Add => {
                let [b, a] = self.pop()?;
                self.s.push_front(a + b);
            }
            Op::Subtract => {
                let [b, a] = self.pop()?;
                self.s.push_front(a - b);
            }
            Op::Multiply => {
                let [b, a] = self.pop()?;
                self.s.push_front(a * b);
            }
            Op::Divide => {
                let [b, a] = self.check_and_pop(|stack: &[BigDecimal; 2]| {
                    if stack[0] == BigDecimal::zero() {
                        Err(StackError::InvalidArgument(
                            "element 1 must be non-zero".into(),
                        ))
                    } else {
                        Ok(())
                    }
                })?;
                self.s.push_front(a / b);
            }
            Op::Modulo => {
                let [b, a] = self.pop()?;
                self.s.push_front(a % b);
            }
            Op::Sqrt => {
                let [a] = self.check_and_pop(|stack: &[BigDecimal; 1]| {
                    if stack[0] < BigDecimal::zero() {
                        Err(StackError::InvalidArgument(
                            "element 1 must be positive".into(),
                        ))
                    } else {
                        Ok(())
                    }
                })?;
                self.s.push_front(a.sqrt().unwrap());
            }
            Op::Pow => {
                let [b, a] = self.check_and_pop(|stack: &[BigDecimal; 2]| {
                    if !(stack[0].is_integer() && stack[0] > BigDecimal::zero()) {
                        return Err(StackError::InvalidArgument(
                            "element 1 must be a positive integer".into(),
                        ));
                    }
                    if !stack[1].is_integer() {
                        return Err(StackError::InvalidArgument(
                            "element 2 must be an integer".into(),
                        ));
                    }
                    Ok(())
                })?;
                let (a, _) = a.as_bigint_and_scale();
                let (b, _) = b.as_bigint_and_scale();
                self.s.push_front(BigDecimal::from_bigint(
                    a.into_owned().pow(b.to_biguint().unwrap()),
                    0,
                ));
            }
            Op::Duplicate => {
                let [a] = self.pop()?;
                self.s.push_front(a.clone());
                self.s.push_front(a);
            }
            Op::Pop => {
                self.pop::<1>()?;
            }
            Op::Precision => {
                let [a] = self.check_and_pop(|stack: &[BigDecimal; 1]| {
                    if stack[0] <= BigDecimal::zero()
                        || stack[0] > i64::MAX.into()
                        || !stack[0].is_integer()
                    {
                        Err(StackError::InvalidArgument(
                            "element 1 must be a positive integer".into(),
                        ))
                    } else {
                        Ok(())
                    }
                })?;
                self.precision = a.to_u64().unwrap();
            }
            Op::Rotate => {
                let [b, a] = self.pop()?;
                self.s.push_front(b);
                self.s.push_front(a);
            }
        }
        Ok(())
    }

    pub fn snapshot(&self) -> Vec<BigDecimal> {
        // Ensure the scale does not exceed the precision, but don't force
        // it on all numbers as displaying 1.0000000000 is annoying.
        self.s
            .iter()
            .map(|v| {
                if v.digits() > self.precision {
                    v.with_scale(self.precision as i64)
                } else {
                    v.clone()
                }
            })
            .collect()
    }

    fn check_and_pop<const C: usize, F: Fn(&[BigDecimal; C]) -> Result<(), StackError>>(
        &mut self,
        validator: F,
    ) -> Result<[BigDecimal; C], StackError> {
        if self.s.len() < C {
            return Err(StackError::MissingValue(C));
        }
        let result = self
            .s
            .range(0..C)
            .cloned()
            .collect::<Vec<BigDecimal>>()
            .try_into()
            .unwrap();
        validator(&result)?;
        self.s.drain(0..C);
        Ok(result)
    }

    fn pop<const C: usize>(&mut self) -> Result<[BigDecimal; C], StackError> {
        self.check_and_pop(|_| Ok(()))
    }
}

impl TryFrom<State> for Stack {
    type Error = ParseBigDecimalError;

    fn try_from(value: State) -> Result<Self, Self::Error> {
        let mut values = vec![];
        for v in value.stack {
            values.push(BigDecimal::from_str(&v)?);
        }
        Ok(Stack::from(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addition() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(10.into()))?;
        s.apply(Op::Push(20.into()))?;
        s.apply(Op::Add)?;
        assert_eq!(s.snapshot(), vec![BigDecimal::from(30)]);
        Ok(())
    }

    #[test]
    fn subtract() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(10.into()))?;
        s.apply(Op::Push(20.into()))?;
        s.apply(Op::Subtract)?;
        assert_eq!(s.snapshot(), vec![BigDecimal::from(-10)]);
        Ok(())
    }

    #[test]
    fn mumltiply() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(10.into()))?;
        s.apply(Op::Push(20.into()))?;
        s.apply(Op::Multiply)?;
        assert_eq!(s.snapshot(), vec![BigDecimal::from(200)]);
        Ok(())
    }

    #[test]
    fn divide() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(20.into()))?;
        s.apply(Op::Push(10.into()))?;
        s.apply(Op::Divide)?;
        assert_eq!(s.snapshot(), vec![BigDecimal::from(2)]);
        Ok(())
    }

    #[test]
    fn divide_by_zero() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(20.into()))?;
        s.apply(Op::Push(0.into()))?;
        assert_eq!(
            s.apply(Op::Divide),
            Err(StackError::InvalidArgument(
                "element 1 must be non-zero".into()
            ))
        );
        Ok(())
    }

    #[test]
    fn rem() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(7.into()))?;
        s.apply(Op::Push(3.into()))?;
        s.apply(Op::Modulo)?;
        assert_eq!(s.snapshot(), vec![BigDecimal::from(1)]);
        Ok(())
    }

    #[test]
    fn sqrt() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(4.into()))?;
        s.apply(Op::Sqrt)?;
        assert_eq!(s.snapshot(), vec![BigDecimal::from(2)]);
        Ok(())
    }

    #[test]
    fn sqrt_of_negative() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push((-4).into()))?;
        assert_eq!(
            s.apply(Op::Sqrt),
            Err(StackError::InvalidArgument(
                "element 1 must be positive".into()
            ))
        );
        Ok(())
    }

    #[test]
    fn pow() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(2.into()))?;
        s.apply(Op::Push(8.into()))?;
        s.apply(Op::Pow)?;
        assert_eq!(s.snapshot(), vec![BigDecimal::from(256)]);
        Ok(())
    }

    #[test]
    fn duplicate() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(1.into()))?;
        s.apply(Op::Duplicate)?;
        assert_eq!(s.snapshot(), vec![BigDecimal::from(1), BigDecimal::from(1)]);
        Ok(())
    }

    #[test]
    fn pop() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(1.into()))?;
        s.apply(Op::Pop)?;
        assert!(s.snapshot().is_empty());
        Ok(())
    }

    #[test]
    fn rotate() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(1.into()))?;
        s.apply(Op::Push(2.into()))?;
        s.apply(Op::Rotate)?;
        assert_eq!(s.snapshot(), vec![BigDecimal::from(1), BigDecimal::from(2)]);
        Ok(())
    }
}
