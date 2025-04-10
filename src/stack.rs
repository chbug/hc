use std::{collections::VecDeque, str::FromStr};

use bigdecimal::{BigDecimal, ParseBigDecimalError, Pow, ToPrimitive, Zero, num_bigint::BigInt};
use thiserror::Error;

use crate::state::State;

pub struct Stack {
    s: VecDeque<BigDecimal>,
    // Precision when taking a snapshot (not of internal representation).
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

// Arbitrarily cap exponentiation to that number of bits to avoid
// slow computations (that are likely to be accidental anyways).
const MAX_BIT_COUNT: u64 = 1024;

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
                let [a, b] = self.pop()?;
                self.s.push_front(a + b);
            }
            Op::Subtract => {
                let [a, b] = self.pop()?;
                self.s.push_front(a - b);
            }
            Op::Multiply => {
                let [a, b] = self.pop()?;
                self.s.push_front(a * b);
            }
            Op::Divide => {
                let [a, b] = self.check_and_pop(|stack: &[BigDecimal; 2]| {
                    if stack[1] == BigDecimal::zero() {
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
                let [a, b] = self.pop()?;
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
                // This is the only operation that needs to crack open the representation.
                // Careful, BigDecimal's scale works not only as the number of digits after
                // the dot, it's really a generalized
                //     int_value . 10^-scale
                let [a, b] = self.prep_and_pop(|stack: &[BigDecimal; 2]| {
                    let [a, b] = stack;
                    if !(b.is_integer() && b > &BigDecimal::zero() && b < &u64::MAX.into()) {
                        return Err(StackError::InvalidArgument(
                            "element 1 must be a positive integer".into(),
                        ));
                    }
                    if !a.is_integer() {
                        return Err(StackError::InvalidArgument(
                            "element 2 must be an integer".into(),
                        ));
                    }
                    // We know the numbers are integers, but we still need to flush all
                    // the digits into the bigint where we can express the Pow operation.
                    let a = a.with_scale(0).as_bigint_and_scale().0.into_owned();
                    let b = b.with_scale(0).as_bigint_and_scale().0.into_owned();
                    // Arbitrarily cap the number of digits of the result to avoid
                    // accidental freeze / memory blowup when pressing ^ too many times.
                    if BigInt::from(a.bits()) * &b > BigInt::from(MAX_BIT_COUNT) {
                        return Err(StackError::InvalidArgument("too big for me".into()));
                    }
                    Ok([a, b])
                })?;
                let result = a.pow(b.to_biguint().unwrap());
                // Normalization ensures the exponent representation is simplified.
                // For instance 10^100 -> (1, -100) after normalization instead of
                // (1e100, 0).
                self.s.push_front(BigDecimal::from_bigint(result, 0));
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
                let [a, b] = self.pop()?;
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
                let (_, scale) = v.as_bigint_and_scale();
                if scale as u64 > self.precision {
                    v.with_scale(self.precision as i64)
                } else {
                    v.clone()
                }
            })
            .collect()
    }

    // Validate a segment of the stack through a user-provided function and return it.
    // Note: the elements are returned in the reverse order of the stack, which is the
    // natural order for running operations.
    fn check_and_pop<const C: usize, F: Fn(&[BigDecimal; C]) -> Result<(), StackError>>(
        &mut self,
        validator: F,
    ) -> Result<[BigDecimal; C], StackError> {
        self.prep_and_pop(move |input| {
            validator(input)?;
            Ok(input.clone())
        })
    }

    // Transform a segment of the stack through a user-provided function and return it.
    // Note: the elements are returned in the reverse order of the stack, which is the
    // natural order for running operations.
    fn prep_and_pop<const C: usize, T, F: Fn(&[BigDecimal; C]) -> Result<[T; C], StackError>>(
        &mut self,
        validator: F,
    ) -> Result<[T; C], StackError> {
        if self.s.len() < C {
            return Err(StackError::MissingValue(C));
        }
        let result = self
            .s
            .range(0..C)
            .rev()
            .cloned()
            .collect::<Vec<BigDecimal>>()
            .try_into()
            .unwrap();
        let result = validator(&result)?;
        self.s.drain(0..C);
        Ok(result)
    }

    // Return a segment of the stack in reverse order.
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
    use bigdecimal::num_bigint::{self};

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

    #[test]
    fn precision() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(1234.into()))?;
        s.apply(Op::Push(2.into()))?;
        s.apply(Op::Precision)?;
        assert_eq!(s.snapshot()[0].to_string(), "1234");
        s.apply(Op::Push(3.into()))?;
        s.apply(Op::Divide)?;
        assert_eq!(s.snapshot()[0].to_string(), "411.33");
        Ok(())
    }

    #[test]
    fn pow_cap() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(2.into()))?;
        s.apply(Op::Push(2000.into()))?;
        assert_eq!(
            s.apply(Op::Pow),
            Err(StackError::InvalidArgument("too big for me".into()))
        );

        Ok(())
    }

    #[test]
    fn pow_representation() -> Result<(), StackError> {
        let mut s = Stack::new();
        s.apply(Op::Push(10.into()))?;
        s.apply(Op::Push(2.into()))?;
        s.apply(Op::Pow)?;
        let r = s.snapshot()[0].clone();
        let (bi, s) = r.as_bigint_and_scale();

        assert_eq!(*bi, BigInt::new(num_bigint::Sign::Plus, vec![100]));
        assert_eq!(s, 0);
        Ok(())
    }
}
