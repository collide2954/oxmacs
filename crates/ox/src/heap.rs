use std::fmt;

use crate::native::Native;
use crate::value::{Kind, Ref, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeapFull;

impl fmt::Display for HeapFull {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("The heap is full")
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pair {
    pub head: Value,
    pub tail: Value,
}

#[derive(Debug)]
struct Arena<T> {
    slots: Vec<Option<T>>,
    free: Vec<u32>,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }
}

impl<T> Arena<T> {
    fn alloc(&mut self, item: T) -> Result<u32, HeapFull> {
        if let Some(index) = self.free.pop()
            && let Some(slot) = self.slots.get_mut(index as usize)
        {
            *slot = Some(item);
            return Ok(index);
        }
        let index = u32::try_from(self.slots.len()).map_err(|_| HeapFull)?;
        self.slots.push(Some(item));
        Ok(index)
    }

    fn get(&self, index: u32) -> Option<&T> {
        self.slots.get(index as usize)?.as_ref()
    }
}

#[derive(Debug, Default)]
pub struct Heap {
    strings: Arena<Box<str>>,
    pairs: Arena<Pair>,
    natives: Arena<Native>,
}

fn reference(value: Value, kind: Kind) -> Option<u32> {
    match value {
        Value::Ref(found) if found.kind == kind => Some(found.index),
        _ => None,
    }
}

impl Heap {
    pub fn string(&mut self, text: &str) -> Result<Value, HeapFull> {
        let index = self.strings.alloc(text.into())?;
        Ok(Value::Ref(Ref {
            kind: Kind::String,
            index,
        }))
    }

    pub fn text(&self, value: Value) -> Option<&str> {
        self.strings
            .get(reference(value, Kind::String)?)
            .map(AsRef::as_ref)
    }

    pub fn pair(&mut self, head: Value, tail: Value) -> Result<Value, HeapFull> {
        let index = self.pairs.alloc(Pair { head, tail })?;
        Ok(Value::Ref(Ref {
            kind: Kind::Pair,
            index,
        }))
    }

    pub fn pair_of(&self, value: Value) -> Option<Pair> {
        self.pairs.get(reference(value, Kind::Pair)?).copied()
    }

    pub fn native(&mut self, native: Native) -> Result<Value, HeapFull> {
        let index = self.natives.alloc(native)?;
        Ok(Value::Ref(Ref {
            kind: Kind::Native,
            index,
        }))
    }

    pub fn native_of(&self, value: Value) -> Option<&Native> {
        self.natives.get(reference(value, Kind::Native)?)
    }

    pub fn list(
        &mut self,
        items: impl DoubleEndedIterator<Item = Value>,
    ) -> Result<Value, HeapFull> {
        items
            .rev()
            .try_fold(Value::Nil, |tail, head| self.pair(head, tail))
    }

    pub fn items(&self, list: Value) -> Vec<Value> {
        let mut items = Vec::new();
        let mut rest = list;
        while let Some(pair) = self.pair_of(rest) {
            items.push(pair.head);
            rest = pair.tail;
        }
        items
    }
}

#[cfg(test)]
mod tests {
    use super::{Heap, Pair};
    use crate::value::Value;

    #[test]
    fn strings_are_allocated_and_read_back() {
        let mut heap = Heap::default();

        let hello = heap.string("hello").unwrap();
        let world = heap.string("wörld").unwrap();

        assert_ne!(hello, world);
        assert_eq!(heap.text(hello), Some("hello"));
        assert_eq!(heap.text(world), Some("wörld"));
        assert_eq!(heap.text(Value::Int(1)), None);
    }

    #[test]
    fn pairs_hold_a_head_and_a_tail() {
        let mut heap = Heap::default();

        let pair = heap.pair(Value::Int(1), Value::Nil).unwrap();

        assert_eq!(
            heap.pair_of(pair),
            Some(Pair {
                head: Value::Int(1),
                tail: Value::Nil
            })
        );
        assert_eq!(heap.pair_of(Value::Nil), None);
        assert_eq!(heap.text(pair), None);
    }

    #[test]
    fn lists_are_chains_of_pairs_ending_in_nil() {
        let mut heap = Heap::default();
        let items = [Value::Int(1), Value::symbol("two"), Value::Float(3.0)];

        let list = heap.list(items.into_iter()).unwrap();

        assert_eq!(heap.items(list), items);
        assert_eq!(heap.list(std::iter::empty()).unwrap(), Value::Nil);
        assert_eq!(heap.items(Value::Nil), []);
    }
}
