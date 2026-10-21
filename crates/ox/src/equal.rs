use crate::heap::Heap;
use crate::value::{Kind, Value};
use crate::vector::Vector;

type Pending = Vec<(Value, Value)>;

fn same_contents(heap: &Heap, a: Value, b: Value) -> bool {
    match (a, b) {
        (Value::Float(left), Value::Float(right)) => left.is_nan() && right.is_nan(),
        (Value::Ref(left), Value::Ref(right)) => match (left.kind, right.kind) {
            (Kind::String, Kind::String) => heap.text(a) == heap.text(b),
            (Kind::BigInt, Kind::BigInt) => heap.bigint_of(a) == heap.bigint_of(b),
            _ => false,
        },
        _ => false,
    }
}

fn vector_and_list(heap: &Heap, vector: Vector, list: Value, pending: &mut Pending) -> bool {
    let mut rest = list;
    for item in vector.items(heap) {
        let Some(pair) = heap.pair_of(rest) else {
            return false;
        };
        pending.push((item, pair.head));
        rest = pair.tail;
    }
    rest == Value::Nil
}

fn is_list(heap: &Heap, value: Value) -> bool {
    value == Value::Nil || heap.pair_of(value).is_some()
}

fn sequences(heap: &Heap, a: Value, b: Value, pending: &mut Pending) -> Option<bool> {
    match (heap.vector_of(a), heap.vector_of(b)) {
        (Some(left), Some(right)) => {
            if left.count != right.count {
                return Some(false);
            }
            pending.extend(left.items(heap).into_iter().zip(right.items(heap)));
            Some(true)
        }
        (Some(vector), None) if is_list(heap, b) => Some(vector_and_list(heap, vector, b, pending)),
        (None, Some(vector)) if is_list(heap, a) => Some(vector_and_list(heap, vector, a, pending)),
        _ => None,
    }
}

pub fn equal(heap: &Heap, a: Value, b: Value) -> bool {
    let mut pending = Vec::new();
    let (mut a, mut b) = (a, b);
    loop {
        if !identical(a, b) {
            if let (Some(left), Some(right)) = (heap.pair_of(a), heap.pair_of(b)) {
                pending.push((left.tail, right.tail));
                (a, b) = (left.head, right.head);
                continue;
            }
            let same =
                sequences(heap, a, b, &mut pending).unwrap_or_else(|| same_contents(heap, a, b));
            if !same {
                return false;
            }
        }
        let Some(next) = pending.pop() else {
            return true;
        };
        (a, b) = next;
    }
}

pub fn identical(a: Value, b: Value) -> bool {
    match (a, b) {
        (Value::Float(left), Value::Float(right)) => left.to_bits() == right.to_bits(),
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::equal;
    use crate::heap::Heap;
    use crate::value::Value;

    #[test]
    fn long_and_deeply_nested_lists_compare_without_recursion() {
        let mut heap = Heap::default();
        let build = |heap: &mut Heap| {
            let long = heap.list((0..100_000).map(Value::Int)).unwrap();
            (0..100_000).try_fold(long, |inner, _| heap.pair(inner, Value::Nil))
        };

        let a = build(&mut heap).unwrap();
        let b = build(&mut heap).unwrap();

        assert!(equal(&heap, a, b));
    }
}
