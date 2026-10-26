use std::cmp::Ordering;

use crate::builtins::numbers::{quick, quick_holds};
use crate::bytecode::Prim;
use crate::heap::Heap;
use crate::value::Value;

pub(crate) fn quick_prim(heap: &Heap, prim: Prim, a: Value, b: Value) -> Option<Value> {
    let both = [a, b];
    match prim {
        Prim::Add => quick(&both, i64::checked_add, |x, y| x + y),
        Prim::Subtract => quick(&both, i64::checked_sub, |x, y| x - y),
        Prim::Multiply => quick(&both, i64::checked_mul, |x, y| x * y),
        Prim::Less => quick_holds(a, b, Ordering::is_lt).map(Value::Bool),
        Prim::LessEqual => quick_holds(a, b, Ordering::is_le).map(Value::Bool),
        Prim::Greater => quick_holds(a, b, Ordering::is_gt).map(Value::Bool),
        Prim::GreaterEqual => quick_holds(a, b, Ordering::is_ge).map(Value::Bool),
        Prim::Equal => match (a, b) {
            (Value::Int(x), Value::Int(y)) => Some(Value::Bool(x == y)),
            (Value::Keyword(x), Value::Keyword(y)) => Some(Value::Bool(x == y)),
            _ => None,
        },
        Prim::Nth => {
            let Value::Int(index) = b else {
                return None;
            };
            let index = usize::try_from(index).ok()?;
            heap.vector_of(a)?.get(heap, index)
        }
    }
}
