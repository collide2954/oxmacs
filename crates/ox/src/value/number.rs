use std::cmp::Ordering;

use num_bigint::BigInt;
use num_traits::{FromPrimitive, ToPrimitive, Zero};

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::value::Value;
use crate::value::print;

#[derive(Debug, Clone)]
pub enum Number {
    Int(i64),
    Big(BigInt),
    Float(f64),
}

#[expect(
    clippy::cast_precision_loss,
    reason = "mixing an integer into float arithmetic rounds it to the nearest float, as in every language"
)]
pub fn to_float(number: i64) -> f64 {
    number as f64
}

fn shrink(number: BigInt) -> Number {
    i64::try_from(&number).map_or(Number::Big(number), Number::Int)
}

impl Number {
    pub fn of(heap: &Heap, name: &str, value: Value) -> Result<Self, Error> {
        match value {
            Value::Int(number) => Ok(Self::Int(number)),
            Value::Float(number) => Ok(Self::Float(number)),
            other => heap
                .bigint_of(other)
                .map(|number| Self::Big(number.clone()))
                .ok_or_else(|| {
                    Error::new(format!(
                        "{name} expects numbers, got {}",
                        print::brief(heap, other)
                    ))
                    .of(ErrorKind::Type)
                }),
        }
    }

    pub fn store(self, heap: &mut Heap) -> Result<Value, Error> {
        match self {
            Self::Int(number) => Ok(Value::Int(number)),
            Self::Float(number) => Ok(Value::Float(number)),
            Self::Big(number) => heap
                .bigint(number)
                .map_err(|full| Error::new(full.to_string())),
        }
    }

    pub fn float(&self) -> f64 {
        match self {
            Self::Int(number) => to_float(*number),
            Self::Big(number) => number.to_f64().unwrap_or(f64::NAN),
            Self::Float(number) => *number,
        }
    }

    fn big(&self) -> Option<BigInt> {
        match self {
            Self::Int(number) => Some(BigInt::from(*number)),
            Self::Big(number) => Some(number.clone()),
            Self::Float(_) => None,
        }
    }

    fn exact(
        &self,
        other: &Self,
        small: fn(i64, i64) -> Option<i64>,
        large: fn(BigInt, BigInt) -> BigInt,
        inexact: fn(f64, f64) -> f64,
    ) -> Self {
        if let (Self::Int(a), Self::Int(b)) = (self, other)
            && let Some(result) = small(*a, *b)
        {
            return Self::Int(result);
        }
        match (self.big(), other.big()) {
            (Some(a), Some(b)) => shrink(large(a, b)),
            _ => Self::Float(inexact(self.float(), other.float())),
        }
    }

    pub fn add(&self, other: &Self) -> Self {
        self.exact(other, i64::checked_add, |a, b| a + b, |a, b| a + b)
    }

    pub fn subtract(&self, other: &Self) -> Self {
        self.exact(other, i64::checked_sub, |a, b| a - b, |a, b| a - b)
    }

    pub fn multiply(&self, other: &Self) -> Self {
        self.exact(other, i64::checked_mul, |a, b| a * b, |a, b| a * b)
    }

    pub fn divide(&self, other: &Self) -> Result<Self, Error> {
        if other.is_zero() {
            return Err(Error::new("Division by zero").of(ErrorKind::ZeroDivision));
        }
        Ok(Self::Float(self.float() / other.float()))
    }

    pub fn floor_divide(&self, other: &Self) -> Result<Self, Error> {
        if other.is_zero() {
            return Err(Error::new("Division by zero").of(ErrorKind::ZeroDivision));
        }
        Ok(self.exact(
            other,
            |a, b| {
                let quotient = a.checked_div(b)?;
                let inexact = a.checked_rem(b)? != 0;
                if inexact && ((a < 0) != (b < 0)) {
                    quotient.checked_sub(1)
                } else {
                    Some(quotient)
                }
            },
            |a, b| {
                let quotient = &a / &b;
                let remainder = &a % &b;
                if !remainder.is_zero() && ((remainder < BigInt::zero()) != (b < BigInt::zero())) {
                    quotient - 1
                } else {
                    quotient
                }
            },
            |a, b| (a / b).floor(),
        ))
    }

    pub fn modulo(&self, other: &Self) -> Result<Self, Error> {
        let quotient = self.floor_divide(other)?;
        Ok(self.subtract(&other.multiply(&quotient)))
    }

    pub fn remainder(&self, other: &Self) -> Result<Self, Error> {
        if other.is_zero() {
            return Err(Error::new("Division by zero").of(ErrorKind::ZeroDivision));
        }
        Ok(self.exact(other, i64::checked_rem, |a, b| a % b, |a, b| a % b))
    }

    pub fn whole(&self, name: &str, how: fn(f64) -> f64) -> Result<Self, Error> {
        let Self::Float(number) = self else {
            return Ok(self.clone());
        };
        let rounded = how(*number);
        if !rounded.is_finite() {
            let shown = if rounded.is_nan() {
                "nan"
            } else if rounded > 0.0 {
                "inf"
            } else {
                "-inf"
            };
            return Err(Error::new(format!("{name} can't make {shown} whole")).of(ErrorKind::Range));
        }
        Ok(BigInt::from_f64(rounded).map_or(Self::Float(rounded), shrink))
    }

    pub fn even(&self) -> Option<bool> {
        match self {
            Self::Int(number) => Some(number % 2 == 0),
            Self::Big(number) => Some((number % 2_u8).is_zero()),
            Self::Float(_) => None,
        }
    }

    pub fn sign(&self) -> Option<Ordering> {
        self.compare(&Self::Int(0))
    }

    pub fn negate(&self) -> Self {
        Self::Int(0).subtract(self)
    }

    pub fn compare(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => Some(a.cmp(b)),
            (Self::Float(_), _) | (_, Self::Float(_)) => self.float().partial_cmp(&other.float()),
            _ => Some(self.big()?.cmp(&other.big()?)),
        }
    }

    fn is_zero(&self) -> bool {
        match self {
            Self::Int(number) => *number == 0,
            Self::Big(number) => number.is_zero(),
            Self::Float(number) => *number == 0.0,
        }
    }
}
