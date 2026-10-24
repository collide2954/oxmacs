use crate::error::Error;
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::native::Control;
use crate::value::Value;
use crate::vm::Runtime;
use crate::vm::build::make_vector;
use crate::vm::routine::{Next, Routine, Started, finished, is_generator};

use super::{int, items, less, plus, smaller, stretch};

fn take(heap: &mut Heap, count: Value, items: &[Value]) -> Result<Vec<Value>, Error> {
    let end = smaller(heap, count, int(items.len()))?;
    stretch(heap, items, Value::Int(0), end)
}

#[derive(Debug)]
struct Pull {
    generator: Value,
    count: Value,
    at: Value,
    out: Vec<Value>,
    pulled: bool,
}

impl Routine for Pull {
    fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        if self.pulled {
            if finished(&runtime.heap, self.generator) {
                return make_vector(&mut runtime.heap, &self.out).map(Next::Return);
            }
            self.out.push(value);
            self.at = plus(&mut runtime.heap, self.at, Value::Int(1))?;
        }
        if !less(&runtime.heap, self.at, self.count)? {
            return make_vector(&mut runtime.heap, &self.out).map(Next::Return);
        }
        self.pulled = true;
        let next = runtime.control_value(Control::Next);
        Ok(Next::Call(next, std::slice::from_ref(&self.generator)))
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.extend([self.generator, self.count, self.at]);
        values.extend(&self.out);
    }
}

pub(super) fn install(isolate: &mut Isolate) {
    isolate.define_routine("take", 2, Some(2), None, |runtime, _, arguments| {
        let (count, coll) = (arguments[0], arguments[1]);
        if is_generator(&runtime.heap, coll) {
            return Ok(Started::Running(Box::new(Pull {
                generator: coll,
                count,
                at: Value::Int(0),
                out: Vec::new(),
                pulled: false,
            })));
        }
        let heap = &mut runtime.heap;
        let items = items(heap, coll)?;
        let taken = take(heap, count, &items)?;
        make_vector(heap, &taken).map(Started::Done)
    });
}
