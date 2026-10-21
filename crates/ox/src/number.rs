use std::cmp::Ordering;

use crate::error::Error;
use crate::heap::Heap;
use crate::print;
use crate::value::Value;

#[derive(Debug, Clone, Copy)]
pub enum Number {
    Int(i64),
    Float(f64),
}

#[expect(
    clippy::cast_precision_loss,
    reason = "mixing an integer into float arithmetic rounds it to the nearest float, as in every language"
)]
pub fn to_float(number: i64) -> f64 {
    number as f64
}

fn overflow() -> Error {
    Error::new("Integer overflow")
}

impl Number {
    pub fn of(heap: &Heap, name: &str, value: Value) -> Result<Self, Error> {
        match value {
            Value::Int(number) => Ok(Self::Int(number)),
            Value::Float(number) => Ok(Self::Float(number)),
            other => Err(Error::new(format!(
                "{name} expects numbers, got {}",
                print::write(heap, other)
            ))),
        }
    }

    pub fn value(self) -> Value {
        match self {
            Self::Int(number) => Value::Int(number),
            Self::Float(number) => Value::Float(number),
        }
    }

    pub fn float(self) -> f64 {
        match self {
            Self::Int(number) => to_float(number),
            Self::Float(number) => number,
        }
    }

    pub fn add(self, other: Self) -> Result<Self, Error> {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => a.checked_add(b).map(Self::Int).ok_or_else(overflow),
            _ => Ok(Self::Float(self.float() + other.float())),
        }
    }

    pub fn subtract(self, other: Self) -> Result<Self, Error> {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => a.checked_sub(b).map(Self::Int).ok_or_else(overflow),
            _ => Ok(Self::Float(self.float() - other.float())),
        }
    }

    pub fn multiply(self, other: Self) -> Result<Self, Error> {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => a.checked_mul(b).map(Self::Int).ok_or_else(overflow),
            _ => Ok(Self::Float(self.float() * other.float())),
        }
    }

    pub fn divide(self, other: Self) -> Result<Self, Error> {
        if other.is_zero() {
            return Err(Error::new("Division by zero"));
        }
        Ok(Self::Float(self.float() / other.float()))
    }

    pub fn floor_divide(self, other: Self) -> Result<Self, Error> {
        if other.is_zero() {
            return Err(Error::new("Division by zero"));
        }
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => {
                let quotient = a.checked_div(b).ok_or_else(overflow)?;
                let inexact = a.checked_rem(b).is_some_and(|rest| rest != 0);
                if inexact && ((a < 0) != (b < 0)) {
                    quotient.checked_sub(1).map(Self::Int).ok_or_else(overflow)
                } else {
                    Ok(Self::Int(quotient))
                }
            }
            _ => Ok(Self::Float((self.float() / other.float()).floor())),
        }
    }

    pub fn negate(self) -> Result<Self, Error> {
        match self {
            Self::Int(number) => number.checked_neg().map(Self::Int).ok_or_else(overflow),
            Self::Float(number) => Ok(Self::Float(-number)),
        }
    }

    pub fn compare(self, other: Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => Some(a.cmp(&b)),
            _ => self.float().partial_cmp(&other.float()),
        }
    }

    fn is_zero(self) -> bool {
        match self {
            Self::Int(number) => number == 0,
            Self::Float(number) => number == 0.0,
        }
    }
}
