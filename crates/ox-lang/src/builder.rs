use std::any::Any;
use std::rc::Rc;

use crate::bytecode::Arity;
use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::{Isolate, only_from_ox, plain, shown_name};
use crate::native::{Drains, Native, NativeFn};
use crate::symbol::Sym;
use crate::value::Value;
use crate::vm::Runtime;
use crate::vm::bind::{Binding, bind_to};
use crate::vm::routine::{Resumable, Started};

#[derive(Debug)]
#[must_use = "a native is only defined once function, host or routine is called"]
pub struct NativeBuilder<'a> {
    isolate: &'a mut Isolate,
    module: String,
    name: String,
    least: usize,
    most: Option<usize>,
    keywords: Vec<Sym>,
    outside: bool,
    suspends: bool,
    drains: Option<Drains>,
}

impl Isolate {
    pub fn native(&mut self, module: &str, name: &str) -> NativeBuilder<'_> {
        NativeBuilder {
            isolate: self,
            module: module.to_owned(),
            name: name.to_owned(),
            least: 0,
            most: Some(0),
            keywords: Vec::new(),
            outside: false,
            suspends: false,
            drains: None,
        }
    }
}

impl NativeBuilder<'_> {
    pub fn arity(self, least: usize, most: Option<usize>) -> Self {
        Self {
            least,
            most,
            ..self
        }
    }

    pub fn keywords(self, names: &[&str]) -> Self {
        Self {
            keywords: names.iter().map(|name| Sym::intern(name)).collect(),
            ..self
        }
    }

    pub fn outside(self) -> Self {
        Self {
            outside: true,
            ..self
        }
    }

    pub fn suspends(self) -> Self {
        Self {
            suspends: true,
            ..self
        }
    }

    pub(crate) fn drains(self, drains: Drains) -> Self {
        Self {
            drains: Some(drains),
            ..self
        }
    }

    pub fn function(
        self,
        function: impl Fn(&mut Heap, &[Value]) -> Result<Value, Error> + 'static,
    ) {
        let function: Rc<NativeFn> = Rc::new(move |heap, _, arguments| function(heap, arguments));
        self.finish(function, None);
    }

    pub fn host<T: Any>(
        self,
        function: impl Fn(&mut Heap, &mut T, &[Value]) -> Result<Value, Error> + 'static,
    ) {
        let missing = format!(
            "{} isn't available here",
            shown_name(&self.module, &self.name)
        );
        let function: Rc<NativeFn> = Rc::new(move |heap, host, arguments| {
            let Some(host) = host.downcast_mut::<T>() else {
                return Err(Error::new(missing.clone()).of(ErrorKind::Refused));
            };
            function(heap, host, arguments)
        });
        self.finish(function, None);
    }

    pub fn routine(
        self,
        start: impl Fn(&mut Runtime, Value, &[Value]) -> Result<Started, Error> + 'static,
    ) {
        let shown = shown_name(&self.module, &self.name);
        let resumable = Rc::new(Resumable::new(Sym::intern(&shown), Box::new(start)));
        self.finish(only_from_ox(&shown), Some(resumable));
    }

    fn finish(self, function: Rc<NativeFn>, resumable: Option<Rc<Resumable>>) {
        let shown = shown_name(&self.module, &self.name);
        let (function, most) = if self.keywords.is_empty() {
            (function, self.most)
        } else {
            let pairs = 2 * self.keywords.len();
            (
                bound(shown, self.least, self.most, &self.keywords, function),
                self.most.map(|most| most + pairs),
            )
        };
        let native = Native {
            suspends: self.suspends,
            outside: self.outside,
            keywords: self.keywords.into(),
            resumable,
            drains: self.drains,
            ..plain(&self.module, &self.name, self.least, most, function)
        };
        self.isolate.install(&self.module, &self.name, native);
    }
}

fn bound(
    shown: String,
    least: usize,
    most: Option<usize>,
    keywords: &[Sym],
    function: Rc<NativeFn>,
) -> Rc<NativeFn> {
    let optional = most.map_or(0, |most| most.saturating_sub(least));
    let arity = Arity {
        required: u16::try_from(least).unwrap_or(u16::MAX),
        optional: u16::try_from(optional).unwrap_or(u16::MAX),
        rest: most.is_none(),
        keywords: keywords.to_vec(),
    };
    let positional = least + optional;
    Rc::new(move |heap, host, arguments| {
        let values = match bind_to(heap, &shown, &arity, arguments)? {
            Binding::Direct => arguments.to_vec(),
            Binding::Bound { values, .. } => values,
        };
        if !arity.rest {
            return function(heap, host, &values);
        }
        let (fixed, after) = values.split_at(positional.min(values.len()));
        let (list, named) = after
            .split_first()
            .map_or((Value::Nil, &[][..]), |(list, named)| (*list, named));
        let mut ordered = fixed.to_vec();
        ordered.extend_from_slice(named);
        ordered.extend(heap.items(list));
        function(heap, host, &ordered)
    })
}
