use std::{collections::VecDeque, str::FromStr};

use bigdecimal::{num_bigint::BigInt, BigDecimal, ParseBigDecimalError, Pow, ToPrimitive, Zero};
use thiserror::Error;

use crate::state::State;

/// Stack represents the internal state of the calculator.
pub struct Stack {
    stack: Undoable<InstantStack>,
}

/// An Undoable keeps track of a sequence of states, and allows
/// to undo/redo them, in the most simple way: it clones the old
/// state into the new one for further manipulation, and keeps
/// an index on the currently active one.
pub struct Undoable<T>
where
    T: Clone,
{
    history: Vec<T>,
    current: usize,
}

impl<T> Undoable<T>
where
    T: Clone,
{
    pub fn new(start: T) -> Undoable<T> {
        Undoable {
            history: vec![start],
            current: 0,
        }
    }

    /// Introduce a new state, identical to the current one.
    pub fn add(&mut self, v: T) -> &mut T {
        self.history.truncate(self.current + 1);
        self.history.push(v);
        self.current += 1;
        &mut (self.history[self.current])
    }

    /// Undo to the previous state if there is one, returns false if not.
    pub fn undo(&mut self) -> bool {
        if self.current == 0 {
            return false;
        }
        self.current -= 1;
        return true;
    }

    /// Redo to the next state if there is one, returns false if not.
    pub fn redo(&mut self) -> bool {
        if self.current >= self.history.len() - 1 {
            return false;
        }
        self.current += 1;
        return true;
    }

    pub fn cur(&self) -> &T {
        &(self.history[self.current])
    }
}

/// Instantaneous stack, without undo/redo support. This is the
/// representation of what's seen by the user at a given point in
/// time.
#[derive(Clone, Debug)]
pub struct InstantStack {
    pub stack: VecDeque<BigDecimal>,
    // Precision when taking a snapshot (not of internal representation).
    pub precision: u64,
}

impl InstantStack {
    pub fn new(stack: VecDeque<BigDecimal>, precision: u64) -> InstantStack {
        InstantStack { stack, precision }
    }

    pub fn push_front(&mut self, v: BigDecimal) {
        self.stack.push_front(v);
    }

    pub fn pop_front(&mut self) -> Option<BigDecimal> {
        self.stack.pop_front()
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
        if self.stack.len() < C {
            return Err(StackError::MissingValue(C));
        }
        let result = self
            .stack
            .range(0..C)
            .rev()
            .cloned()
            .collect::<Vec<BigDecimal>>()
            .try_into()
            .unwrap();
        let result = validator(&result)?;
        self.stack.drain(0..C);
        Ok(result)
    }

    // Return a segment of the stack in reverse order.
    fn pop<const C: usize>(&mut self) -> Result<[BigDecimal; C], StackError> {
        self.check_and_pop(|_| Ok(()))
    }
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
    Undo,
    Redo,
}

// Arbitrarily cap exponentiation to that number of bits to avoid
// slow computations (that are likely to be accidental anyways).
const MAX_BIT_COUNT: u64 = 1024;
const DEFAULT_PRECISION: u64 = 12;

impl Stack {
    #[cfg(test)]
    pub fn new() -> Stack {
        Stack {
            stack: Undoable::new(InstantStack::new(VecDeque::new(), DEFAULT_PRECISION)),
        }
    }

    pub fn from(values: Vec<BigDecimal>, precision: Option<u64>) -> Stack {
        Stack {
            stack: Undoable::new(InstantStack::new(
                values.into(),
                precision.unwrap_or(DEFAULT_PRECISION),
            )),
        }
    }

    pub fn apply(&mut self, op: Op) -> Result<(), StackError> {
        match op {
            Op::Undo => match self.stack.undo() {
                true => Ok(()),
                false => Err(StackError::InvalidArgument("Nothing to undo.".to_owned())),
            },
            Op::Redo => match self.stack.redo() {
                true => Ok(()),
                false => Err(StackError::InvalidArgument("Nothing to redo.".to_owned())),
            },
            op => {
                let mut s = self.stack.cur().clone();
                match apply_on_stack(&mut s, op) {
                    Ok(_) => {
                        self.stack.add(s);
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }

    pub fn snapshot(&self) -> Vec<BigDecimal> {
        // Ensure the scale does not exceed the precision, but don't force
        // it on all numbers as displaying 1.0000000000 is annoying.
        let cur = self.stack.cur();
        cur.stack
            .iter()
            .map(|v| {
                let (_, scale) = v.as_bigint_and_scale();
                if scale as u64 > cur.precision {
                    v.with_scale(cur.precision as i64)
                } else {
                    v.clone()
                }
            })
            .collect()
    }

    pub fn edit_top(&mut self) -> Option<BigDecimal> {
        // TODO: this is actually a bit subboptimal, as we introduce a new
        // state with the edited item being removed, which is then visible
        // in the history.
        let cur = self.stack.add(self.stack.cur().clone());
        cur.pop_front()
    }

    // Return the precision of the display.
    pub fn precision(&self) -> u64 {
        self.stack.cur().precision
    }
}

impl TryFrom<State> for Stack {
    type Error = ParseBigDecimalError;

    fn try_from(value: State) -> Result<Self, Self::Error> {
        let mut values = vec![];
        for v in value.stack {
            values.push(BigDecimal::from_str(&v)?);
        }
        Ok(Stack::from(values, value.precision))
    }
}

fn apply_on_stack(s: &mut InstantStack, op: Op) -> Result<(), StackError> {
    match op {
        // Undo & Redo are meta-operations.
        Op::Undo | Op::Redo => {}
        Op::Push(v) => {
            s.push_front(v);
        }
        Op::Add => {
            let [a, b] = s.pop()?;
            s.push_front(a + b);
        }
        Op::Subtract => {
            let [a, b] = s.pop()?;
            s.push_front(a - b);
        }
        Op::Multiply => {
            let [a, b] = s.pop()?;
            s.push_front(a * b);
        }
        Op::Divide => {
            let [a, b] = s.check_and_pop(|stack: &[BigDecimal; 2]| {
                if stack[1] == BigDecimal::zero() {
                    Err(StackError::InvalidArgument(
                        "element 1 must be non-zero".into(),
                    ))
                } else {
                    Ok(())
                }
            })?;
            s.push_front(a / b);
        }
        Op::Modulo => {
            let [a, b] = s.pop()?;
            s.push_front(a % b);
        }
        Op::Sqrt => {
            let [a] = s.check_and_pop(|stack: &[BigDecimal; 1]| {
                if stack[0] < BigDecimal::zero() {
                    Err(StackError::InvalidArgument(
                        "element 1 must be positive".into(),
                    ))
                } else {
                    Ok(())
                }
            })?;
            s.push_front(a.sqrt().unwrap());
        }
        Op::Pow => {
            // This is the only operation that needs to crack open the representation.
            // Careful, BigDecimal's scale works not only as the number of digits after
            // the dot, it's really a generalized
            //     int_value . 10^-scale
            let [a, b] = s.prep_and_pop(|stack: &[BigDecimal; 2]| {
                let [a, b] = stack;
                if !(b.is_integer() && b > &BigDecimal::zero() && b < &BigDecimal::from(u64::MAX)) {
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
            s.push_front(BigDecimal::from_bigint(result, 0));
        }
        Op::Duplicate => {
            let [a] = s.pop()?;
            s.push_front(a.clone());
            s.push_front(a);
        }
        Op::Pop => {
            s.pop::<1>()?;
        }
        Op::Precision => {
            let [a] = s.check_and_pop(|stack: &[BigDecimal; 1]| {
                if stack[0] <= BigDecimal::zero()
                    || stack[0] > BigDecimal::from(i64::MAX)
                    || !stack[0].is_integer()
                {
                    Err(StackError::InvalidArgument(
                        "element 1 must be a positive integer".into(),
                    ))
                } else {
                    Ok(())
                }
            })?;
            s.precision = a.to_u64().unwrap();
        }
        Op::Rotate => {
            let [a, b] = s.pop()?;
            s.push_front(b);
            s.push_front(a);
        }
    }
    Ok(())
}

#[cfg(test)]
mod undoable_tests {
    use super::*;

    #[test]
    fn empty() {
        let mut u: Undoable<i32> = Undoable::new(0);
        assert!(!u.undo());
        assert!(!u.redo());
    }

    #[test]
    fn add() {
        let mut u: Undoable<i32> = Undoable::new(0);
        let new = u.add(1);
        assert_eq!(1, *new);
        assert_eq!(1, *u.cur());
        let new = u.add(2);
        assert_eq!(2, *new);
        assert_eq!(2, *u.cur());
    }

    #[test]
    fn undo() {
        let mut u: Undoable<i32> = Undoable::new(0);
        u.add(1);
        assert_eq!(1, *u.cur());
        // Undo leads to the previous value.
        assert!(u.undo());
        assert_eq!(0, *u.cur());
        // ...and fwd from there ignores the previous value.
        u.add(2);
        assert_eq!(2, *u.cur());
    }

    #[test]
    fn redo() {
        let mut u: Undoable<i32> = Undoable::new(0);
        u.add(1);
        // Undo leads to the previous value.
        assert!(u.undo());
        assert_eq!(0, *u.cur());
        // ...and redo brings back the most recent one.
        assert!(u.redo());
        assert_eq!(1, *u.cur());
    }
}

#[cfg(test)]
mod stack_tests {
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
