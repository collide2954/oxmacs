use std::any::Any;
use std::fmt;
use std::ops::Range;
use std::rc::Rc;

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::symbol::Sym;
use crate::value::Value;
use crate::vm::routine::Resumable;

pub type NativeFn = dyn Fn(&mut Heap, &mut dyn Any, &[Value]) -> Result<Value, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Control {
    Yield,
    Next,
    Await,
    Generator,
    Spawn,
    Send,
    Receive,
    Worker,
    Sleep,
    AtomicEnter,
    AtomicLeave,
    ProfileStart,
    ProfileStop,
    ProfileSnapshot,
}

#[derive(Clone)]
pub struct Native {
    pub name: Sym,
    pub least: usize,
    pub most: Option<usize>,
    pub function: Rc<NativeFn>,
    pub control: Option<Control>,
    pub awaits: bool,
    pub outside: bool,
    pub keywords: Box<[Sym]>,
    pub(crate) resumable: Option<Rc<Resumable>>,
    pub(crate) drains: Option<Drains>,
    pub(crate) captured: Box<[Value]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drains {
    At(usize),
    From(usize),
    Last,
}

impl Drains {
    pub(crate) fn range(self, count: usize) -> Range<usize> {
        match self {
            Self::At(at) => at.min(count)..(at + 1).min(count),
            Self::From(at) => at.min(count)..count,
            Self::Last => count.saturating_sub(1)..count,
        }
    }
}

impl fmt::Debug for Native {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Native")
            .field("name", &self.name.name())
            .finish_non_exhaustive()
    }
}

pub fn given<'a>(
    names: &'a [&'a str],
    slots: &'a [Value],
) -> impl Iterator<Item = (&'a str, Value)> + 'a {
    names
        .iter()
        .zip(slots)
        .filter(|(_, value)| **value != Value::Nil)
        .map(|(name, value)| (*name, *value))
}

pub fn arguments(count: usize) -> String {
    if count == 1 {
        "1 argument".to_owned()
    } else {
        format!("{count} arguments")
    }
}

impl Native {
    pub(crate) fn parameters(&self) -> Option<&Rc<str>> {
        self.resumable.as_ref()?.parameters.as_ref()
    }

    pub fn check(&self, count: usize) -> Result<(), Error> {
        if !self.keywords.is_empty() {
            return Ok(());
        }
        let fits = count >= self.least && self.most.is_none_or(|most| count <= most);
        if fits {
            return Ok(());
        }
        let name = self.name.name();
        let wanted = match self.most {
            Some(most) if most == self.least => arguments(most),
            Some(most) => format!("{} to {most} arguments", self.least),
            None => format!("at least {}", arguments(self.least)),
        };
        Err(Error::new(format!("{name} takes {wanted}, got {count}")).of(ErrorKind::Arity))
    }
}
