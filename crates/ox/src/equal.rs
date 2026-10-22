use std::hash::{DefaultHasher, Hash, Hasher};

use crate::heap::Heap;
use crate::value::{Kind, Value};
use crate::vector::Vector;

type Pending = Vec<(Value, Value)>;

const SEQUENCE: u64 = 0x9e37_79b9_7f4a_7c15;
const DOTTED: u64 = 0x2545_f491_4f6c_dd1d;

enum Task {
    Hash(Value),
    Sequence { count: usize, dotted: bool },
    Unordered { count: usize, tag: u64 },
}

fn mix(bits: u64) -> u64 {
    let mut bits = bits;
    bits ^= bits >> 30;
    bits = bits.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    bits ^= bits >> 27;
    bits = bits.wrapping_mul(0x94d0_49bb_1331_11eb);
    bits ^ (bits >> 31)
}

fn tagged(tag: u64, bits: u64) -> u64 {
    mix(bits ^ tag.wrapping_mul(SEQUENCE))
}

fn digest(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn scalar(heap: &Heap, value: Value) -> u64 {
    match value {
        Value::Nil => SEQUENCE,
        Value::Bool(flag) => tagged(1, u64::from(flag)),
        Value::Int(number) => tagged(2, u64::from_ne_bytes(number.to_ne_bytes())),
        Value::Float(number) => tagged(
            3,
            if number.is_nan() {
                u64::MAX
            } else {
                number.to_bits()
            },
        ),
        Value::Char(character) => tagged(4, u64::from(character)),
        Value::Symbol(name) => tagged(5, digest(name)),
        Value::Keyword(name) => tagged(6, digest(name)),
        Value::Ref(reference) => {
            if let Some(hash) = heap.string_hash(value) {
                tagged(7, hash)
            } else if let Some(number) = heap.bigint_of(value) {
                tagged(8, digest(number))
            } else {
                tagged(9, digest(reference))
            }
        }
    }
}

fn sequence_items(heap: &Heap, value: Value) -> Option<(Vec<Value>, Option<Value>)> {
    if let Some(vector) = heap.vector_of(value) {
        return Some((vector.items(heap), None));
    }
    heap.pair_of(value)?;
    let mut items = Vec::new();
    let mut rest = value;
    while let Some(pair) = heap.pair_of(rest) {
        items.push(pair.head);
        rest = pair.tail;
    }
    Some((items, (rest != Value::Nil).then_some(rest)))
}

pub fn hash(heap: &Heap, value: Value) -> u64 {
    let mut tasks = vec![Task::Hash(value)];
    let mut results = Vec::new();
    while let Some(task) = tasks.pop() {
        match task {
            Task::Hash(value) => {
                if let Some(set) = heap.set_of(value) {
                    let items = set.entries(heap);
                    tasks.push(Task::Unordered {
                        count: items.len(),
                        tag: 11,
                    });
                    tasks.extend(items.into_iter().map(|(item, _)| Task::Hash(item)));
                    continue;
                }
                if let Some(map) = heap.map_of(value) {
                    let entries = map.entries(heap);
                    tasks.push(Task::Unordered {
                        count: entries.len(),
                        tag: 10,
                    });
                    for (key, value) in entries.into_iter().rev() {
                        tasks.push(Task::Sequence {
                            count: 2,
                            dotted: false,
                        });
                        tasks.push(Task::Hash(value));
                        tasks.push(Task::Hash(key));
                    }
                    continue;
                }
                let Some((items, tail)) = sequence_items(heap, value) else {
                    results.push(scalar(heap, value));
                    continue;
                };
                tasks.push(Task::Sequence {
                    count: items.len() + usize::from(tail.is_some()),
                    dotted: tail.is_some(),
                });
                tasks.extend(tail.map(Task::Hash));
                tasks.extend(items.into_iter().rev().map(Task::Hash));
            }
            Task::Sequence { count, dotted } => {
                let start = results.len().saturating_sub(count);
                let combined = results.drain(start..).fold(SEQUENCE, |total, item| {
                    mix(total.wrapping_mul(31).wrapping_add(item))
                });
                results.push(if dotted {
                    mix(combined ^ DOTTED)
                } else {
                    combined
                });
            }
            Task::Unordered { count, tag } => {
                let start = results.len().saturating_sub(count);
                let combined = results.drain(start..).fold(0, u64::wrapping_add);
                results.push(tagged(tag, combined));
            }
        }
    }
    results.pop().unwrap_or(SEQUENCE)
}

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

fn sets(heap: &Heap, a: Value, b: Value) -> Option<bool> {
    let (left, right) = (heap.set_of(a)?, heap.set_of(b)?);
    Some(
        left.count == right.count
            && left
                .entries(heap)
                .into_iter()
                .all(|(item, _)| right.get(heap, item).is_some()),
    )
}

fn maps(heap: &Heap, a: Value, b: Value, pending: &mut Pending) -> Option<bool> {
    let (left, right) = (heap.map_of(a)?, heap.map_of(b)?);
    if left.count != right.count {
        return Some(false);
    }
    for (key, value) in left.entries(heap) {
        let Some(other) = right.get(heap, key) else {
            return Some(false);
        };
        pending.push((value, other));
    }
    Some(true)
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
            let same = sequences(heap, a, b, &mut pending)
                .or_else(|| maps(heap, a, b, &mut pending))
                .or_else(|| sets(heap, a, b))
                .unwrap_or_else(|| same_contents(heap, a, b));
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
    use super::{equal, hash};
    use crate::heap::Heap;
    use crate::value::Value;

    fn deep(heap: &mut Heap) -> Value {
        let long = heap.list((0..100_000).map(Value::Int)).unwrap();
        (0..100_000)
            .try_fold(long, |inner, _| heap.pair(inner, Value::Nil))
            .unwrap()
    }

    #[test]
    fn long_and_deeply_nested_lists_compare_without_recursion() {
        let mut heap = Heap::default();

        let a = deep(&mut heap);
        let b = deep(&mut heap);

        assert!(equal(&heap, a, b));
    }

    #[test]
    fn long_and_deeply_nested_lists_hash_without_recursion() {
        let mut heap = Heap::default();

        let a = deep(&mut heap);
        let b = deep(&mut heap);

        assert_eq!(hash(&heap, a), hash(&heap, b));
    }

    #[test]
    fn dotted_pairs_hash_by_their_head_and_tail() {
        let mut heap = Heap::default();
        let a = heap.pair(Value::Int(1), Value::Int(2)).unwrap();
        let b = heap.pair(Value::Int(1), Value::Int(2)).unwrap();
        let list = heap
            .list([Value::Int(1), Value::Int(2)].into_iter())
            .unwrap();

        assert_eq!(hash(&heap, a), hash(&heap, b));
        assert_ne!(hash(&heap, a), hash(&heap, list));
    }
}
