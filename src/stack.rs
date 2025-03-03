use std::{collections::VecDeque, str::FromStr};

use bigdecimal::{BigDecimal, BigDecimalRef, ParseBigDecimalError};
use thiserror::Error;

use crate::state::State;

pub struct Stack {
    s: VecDeque<BigDecimal>,
}

#[derive(Error, Debug)]
pub enum StackError {
    #[error("Operation requires {0} elements")]
    MissingValue(usize),
}

#[derive(Debug, Clone)]
pub enum Op {
    Push(BigDecimal),
    Add,
    Subtract,
    Multiply,
    Divide,
    Duplicate,
    Pop,
}

impl Stack {
    #[cfg(test)]
    pub fn new() -> Stack {
        Stack { s: VecDeque::new() }
    }

    pub fn from(values: Vec<BigDecimal>) -> Stack {
        Stack { s: values.into() }
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
                let [b, a] = self.pop()?;
                self.s.push_front(a / b);
            }
            Op::Duplicate => {
                let [a] = self.pop()?;
                self.s.push_front(a.clone());
                self.s.push_front(a);
            }
            Op::Pop => {
                self.pop::<1>()?;
            }
        }
        Ok(())
    }

    pub fn snapshot(&self) -> Vec<BigDecimalRef> {
        self.s.iter().map(|v| v.to_ref()).collect()
    }

    fn pop<const C: usize>(&mut self) -> Result<[BigDecimal; C], StackError> {
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
        self.s.drain(0..C);
        Ok(result)
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
        assert_eq!(s.snapshot(), vec![BigDecimal::from(30).to_ref()]);
        Ok(())
    }

    #[test]
    fn subtract() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(10.into()))?;
        s.apply(Op::Push(20.into()))?;
        s.apply(Op::Subtract)?;
        assert_eq!(s.snapshot(), vec![BigDecimal::from(-10).to_ref()]);
        Ok(())
    }

    #[test]
    fn mumltiply() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(10.into()))?;
        s.apply(Op::Push(20.into()))?;
        s.apply(Op::Multiply)?;
        assert_eq!(s.snapshot(), vec![BigDecimal::from(200).to_ref()]);
        Ok(())
    }

    #[test]
    fn divide() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(20.into()))?;
        s.apply(Op::Push(10.into()))?;
        s.apply(Op::Divide)?;
        assert_eq!(s.snapshot(), vec![BigDecimal::from(2).to_ref()]);
        Ok(())
    }

    #[test]
    fn duplicate() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(1.into()))?;
        s.apply(Op::Duplicate)?;
        assert_eq!(
            s.snapshot(),
            vec![BigDecimal::from(1).to_ref(), BigDecimal::from(1).to_ref()]
        );
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
}
