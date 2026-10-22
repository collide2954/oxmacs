use std::fmt;
use std::rc::Rc;

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::symbol::Sym;
use crate::value::Value;

pub type NativeFn = dyn Fn(&mut Heap, &[Value]) -> Result<Value, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Yield,
    Next,
    Await,
    Generator,
    Spawn,
    Send,
    Receive,
    Worker,
}

#[derive(Clone)]
pub struct Native {
    pub name: Sym,
    pub least: usize,
    pub most: Option<usize>,
    pub function: Rc<NativeFn>,
    pub control: Option<Control>,
}

impl fmt::Debug for Native {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Native")
            .field("name", &self.name.name())
            .finish_non_exhaustive()
    }
}

pub fn arguments(count: usize) -> String {
    if count == 1 {
        "1 argument".to_owned()
    } else {
        format!("{count} arguments")
    }
}

impl Native {
    pub fn check(&self, count: usize) -> Result<(), Error> {
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
