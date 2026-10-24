use std::any::Any;
use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use num_bigint::BigInt;

use crate::bytecode::Code;
use crate::native::Native;
use crate::portable::{Channel, Signal};
use crate::read::Marks;
use crate::span::Span;
use crate::symbol::Sym;
use crate::task::TaskId;
use crate::value::foreign::{Foreign, Holder};
use crate::value::map::{Map, Node};
use crate::value::promise::Promise;
use crate::value::record::{Family, Record, RecordType};
use crate::value::vector::Vector;
use crate::value::{Kind, Ref, Value};
use crate::vm::machine::Generator;

mod gc;
mod jobs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeapFull;

#[derive(Debug, Clone)]
pub struct Rooted(Rc<Cell<Value>>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hold(u64);

impl Rooted {
    #[must_use]
    pub fn get(&self) -> Value {
        self.0.get()
    }
}

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
struct OxString {
    text: Box<str>,
    length: usize,
    hash: u64,
}

#[derive(Debug, Clone)]
pub struct Identifier {
    pub name: Sym,
    pub marks: Marks,
    pub span: Span,
}

#[derive(Debug)]
pub struct Closure {
    pub code: Rc<Code>,
    pub captured: Box<[Value]>,
}

#[derive(Debug)]
struct Arena<T> {
    slots: Vec<Option<T>>,
    free: Vec<u32>,
    marks: Vec<bool>,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            marks: Vec::new(),
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

    fn get_mut(&mut self, index: u32) -> Option<&mut T> {
        self.slots.get_mut(index as usize)?.as_mut()
    }

    fn in_use(&self) -> usize {
        self.slots.len() - self.free.len()
    }

    fn unmark(&mut self) {
        self.marks.clear();
        self.marks.resize(self.slots.len(), false);
    }

    fn mark(&mut self, index: u32) -> bool {
        match self.marks.get_mut(index as usize) {
            Some(mark) if !*mark => {
                *mark = true;
                true
            }
            _ => false,
        }
    }

    fn sweep(&mut self) -> usize {
        let mut freed = 0;
        for (index, (slot, marked)) in (0_u32..).zip(self.slots.iter_mut().zip(&self.marks)) {
            if slot.is_some() && !*marked {
                *slot = None;
                self.free.push(index);
                freed += 1;
            }
        }
        freed
    }
}

#[derive(Debug, Default)]
pub struct Heap {
    strings: Arena<OxString>,
    pairs: Arena<Pair>,
    natives: Arena<Native>,
    closures: Arena<Closure>,
    cells: Arena<Value>,
    bigints: Arena<BigInt>,
    vectors: Arena<Vector>,
    nodes: Arena<Box<[Value]>>,
    maps: Arena<Map>,
    map_nodes: Arena<Node>,
    sets: Arena<Map>,
    record_types: Arena<RecordType>,
    records: Arena<Record>,
    atoms: Arena<Value>,
    identifiers: Arena<Identifier>,
    generators: Arena<Generator>,
    promises: Arena<Promise>,
    channels: Arena<Arc<Channel>>,
    foreigns: Arena<Foreign>,
    roots: Vec<Weak<Cell<Value>>>,
    held: HashMap<Hold, Value>,
    next_hold: u64,
    atom_writes: u64,
    jobs: Vec<jobs::Job>,
    signal: Arc<Signal>,
    next_collection: usize,
    running: Option<TaskId>,
}

fn reference(value: Value, kind: Kind) -> Option<u32> {
    match value {
        Value::Ref(found) if found.kind == kind => Some(found.index),
        _ => None,
    }
}

impl Heap {
    pub fn running_task(&self) -> Option<TaskId> {
        self.running
    }

    pub(crate) fn set_running_task(&mut self, task: Option<TaskId>) {
        self.running = task;
    }

    pub fn hold(&mut self, value: Value) -> Hold {
        self.next_hold += 1;
        let hold = Hold(self.next_hold);
        self.held.insert(hold, value);
        hold
    }

    pub fn held(&self, hold: Hold) -> Option<Value> {
        self.held.get(&hold).copied()
    }

    pub fn release(&mut self, hold: Hold) -> Option<Value> {
        self.held.remove(&hold)
    }

    pub fn root(&mut self, value: Value) -> Rooted {
        let cell = Rc::new(Cell::new(value));
        self.roots.push(Rc::downgrade(&cell));
        Rooted(cell)
    }

    pub fn string(&mut self, text: &str) -> Result<Value, HeapFull> {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let index = self.strings.alloc(OxString {
            text: text.into(),
            length: text.chars().count(),
            hash: hasher.finish(),
        })?;
        Ok(Value::Ref(Ref {
            kind: Kind::String,
            index,
        }))
    }

    pub fn text(&self, value: Value) -> Option<&str> {
        self.strings
            .get(reference(value, Kind::String)?)
            .map(|string| string.text.as_ref())
    }

    pub fn length(&self, value: Value) -> Option<usize> {
        self.strings
            .get(reference(value, Kind::String)?)
            .map(|string| string.length)
    }

    pub fn string_hash(&self, value: Value) -> Option<u64> {
        self.strings
            .get(reference(value, Kind::String)?)
            .map(|string| string.hash)
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

    pub fn closure(&mut self, closure: Closure) -> Result<Value, HeapFull> {
        let index = self.closures.alloc(closure)?;
        Ok(Value::Ref(Ref {
            kind: Kind::Closure,
            index,
        }))
    }

    pub fn closure_of(&self, value: Value) -> Option<&Closure> {
        self.closures.get(reference(value, Kind::Closure)?)
    }

    pub fn cell(&mut self, value: Value) -> Result<Value, HeapFull> {
        let index = self.cells.alloc(value)?;
        Ok(Value::Ref(Ref {
            kind: Kind::Cell,
            index,
        }))
    }

    pub fn cell_get(&self, cell: Value) -> Option<Value> {
        self.cells.get(reference(cell, Kind::Cell)?).copied()
    }

    pub fn cell_set(&mut self, cell: Value, value: Value) -> bool {
        let Some(slot) = reference(cell, Kind::Cell).and_then(|index| self.cells.get_mut(index))
        else {
            return false;
        };
        *slot = value;
        true
    }

    pub fn bigint(&mut self, number: BigInt) -> Result<Value, HeapFull> {
        let index = self.bigints.alloc(number)?;
        Ok(Value::Ref(Ref {
            kind: Kind::BigInt,
            index,
        }))
    }

    pub fn bigint_of(&self, value: Value) -> Option<&BigInt> {
        self.bigints.get(reference(value, Kind::BigInt)?)
    }

    pub fn vector(&mut self, vector: Vector) -> Result<Value, HeapFull> {
        let index = self.vectors.alloc(vector)?;
        Ok(Value::Ref(Ref {
            kind: Kind::Vector,
            index,
        }))
    }

    pub fn vector_of(&self, value: Value) -> Option<Vector> {
        self.vectors.get(reference(value, Kind::Vector)?).copied()
    }

    pub fn node(&mut self, items: Box<[Value]>) -> Result<Value, HeapFull> {
        let index = self.nodes.alloc(items)?;
        Ok(Value::Ref(Ref {
            kind: Kind::VecNode,
            index,
        }))
    }

    pub fn node_items(&self, node: Value) -> &[Value] {
        reference(node, Kind::VecNode)
            .and_then(|index| self.nodes.get(index))
            .map_or(&[], AsRef::as_ref)
    }

    pub fn map(&mut self, map: Map) -> Result<Value, HeapFull> {
        let index = self.maps.alloc(map)?;
        Ok(Value::Ref(Ref {
            kind: Kind::Map,
            index,
        }))
    }

    pub fn map_of(&self, value: Value) -> Option<Map> {
        self.maps.get(reference(value, Kind::Map)?).copied()
    }

    pub fn map_node(&mut self, node: Node) -> Result<Value, HeapFull> {
        let index = self.map_nodes.alloc(node)?;
        Ok(Value::Ref(Ref {
            kind: Kind::MapNode,
            index,
        }))
    }

    pub fn map_node_of(&self, value: Value) -> Option<&Node> {
        self.map_nodes.get(reference(value, Kind::MapNode)?)
    }

    pub fn set(&mut self, set: Map) -> Result<Value, HeapFull> {
        let index = self.sets.alloc(set)?;
        Ok(Value::Ref(Ref {
            kind: Kind::Set,
            index,
        }))
    }

    pub fn set_of(&self, value: Value) -> Option<Map> {
        self.sets.get(reference(value, Kind::Set)?).copied()
    }

    pub fn record_type(&mut self, kind: RecordType) -> Result<Value, HeapFull> {
        let index = self.record_types.alloc(kind)?;
        Ok(Value::Ref(Ref {
            kind: Kind::RecordType,
            index,
        }))
    }

    pub fn record_type_of(&self, value: Value) -> Option<&RecordType> {
        self.record_types.get(reference(value, Kind::RecordType)?)
    }

    pub fn set_constructor(&mut self, kind: Value, constructor: Value) -> bool {
        let Some(found) =
            reference(kind, Kind::RecordType).and_then(|index| self.record_types.get_mut(index))
        else {
            return false;
        };
        found.constructor = constructor;
        true
    }

    pub fn identifier(&mut self, identifier: Identifier) -> Result<Value, HeapFull> {
        let index = self.identifiers.alloc(identifier)?;
        Ok(Value::Ref(Ref {
            kind: Kind::Identifier,
            index,
        }))
    }

    pub fn identifier_of(&self, value: Value) -> Option<&Identifier> {
        self.identifiers.get(reference(value, Kind::Identifier)?)
    }

    pub fn atom(&mut self, value: Value) -> Result<Value, HeapFull> {
        let index = self.atoms.alloc(value)?;
        Ok(Value::Ref(Ref {
            kind: Kind::Atom,
            index,
        }))
    }

    pub fn atom_get(&self, atom: Value) -> Option<Value> {
        self.atoms.get(reference(atom, Kind::Atom)?).copied()
    }

    pub fn atom_set(&mut self, atom: Value, value: Value) -> bool {
        let Some(slot) = reference(atom, Kind::Atom).and_then(|index| self.atoms.get_mut(index))
        else {
            return false;
        };
        *slot = value;
        self.atom_writes += 1;
        true
    }

    pub fn atom_writes(&self) -> u64 {
        self.atom_writes
    }

    pub fn set_family(&mut self, kind: Value, family: Family) -> bool {
        let Some(found) =
            reference(kind, Kind::RecordType).and_then(|index| self.record_types.get_mut(index))
        else {
            return false;
        };
        found.family = family;
        true
    }

    pub fn record(&mut self, record: Record) -> Result<Value, HeapFull> {
        let index = self.records.alloc(record)?;
        Ok(Value::Ref(Ref {
            kind: Kind::Record,
            index,
        }))
    }

    pub fn record_set(&mut self, value: Value, slot: usize, field: Value) -> bool {
        let Some(place) = reference(value, Kind::Record)
            .and_then(|index| self.records.get_mut(index))
            .and_then(|record| record.fields.get_mut(slot))
        else {
            return false;
        };
        *place = field;
        true
    }

    pub fn record_of(&self, value: Value) -> Option<&Record> {
        self.records.get(reference(value, Kind::Record)?)
    }

    pub(crate) fn generator(&mut self, generator: Generator) -> Result<Value, HeapFull> {
        let index = self.generators.alloc(generator)?;
        Ok(Value::Ref(Ref {
            kind: Kind::Generator,
            index,
        }))
    }

    pub(crate) fn generator_of(&self, value: Value) -> Option<&Generator> {
        self.generators.get(reference(value, Kind::Generator)?)
    }

    pub(crate) fn generator_mut(&mut self, value: Value) -> Option<&mut Generator> {
        self.generators.get_mut(reference(value, Kind::Generator)?)
    }

    pub fn new_promise(&mut self) -> Result<Value, HeapFull> {
        self.promise(Promise::Pending { awaited: false })
    }

    pub(crate) fn promise(&mut self, promise: Promise) -> Result<Value, HeapFull> {
        let index = self.promises.alloc(promise)?;
        Ok(Value::Ref(Ref {
            kind: Kind::Promise,
            index,
        }))
    }

    pub(crate) fn promise_of(&self, value: Value) -> Option<Promise> {
        self.promises.get(reference(value, Kind::Promise)?).copied()
    }

    pub(crate) fn channel(&mut self, channel: Arc<Channel>) -> Result<Value, HeapFull> {
        let index = self.channels.alloc(channel)?;
        Ok(Value::Ref(Ref {
            kind: Kind::Channel,
            index,
        }))
    }

    pub(crate) fn channel_of(&self, value: Value) -> Option<&Arc<Channel>> {
        self.channels.get(reference(value, Kind::Channel)?)
    }

    fn holding(&mut self, name: Sym, holder: Holder) -> Result<Value, HeapFull> {
        let index = self.foreigns.alloc(Foreign { name, holder })?;
        Ok(Value::Ref(Ref {
            kind: Kind::Foreign,
            index,
        }))
    }

    pub fn foreign<T: Any>(&mut self, name: &str, value: T) -> Result<Value, HeapFull> {
        self.holding(Sym::intern(name), Holder::Local(Rc::new(value)))
    }

    pub fn shared<T: Any + Send + Sync>(
        &mut self,
        name: &str,
        value: Arc<T>,
    ) -> Result<Value, HeapFull> {
        self.holding(Sym::intern(name), Holder::Shared(value))
    }

    pub(crate) fn shared_foreign(
        &mut self,
        name: Sym,
        value: Arc<dyn Any + Send + Sync>,
    ) -> Result<Value, HeapFull> {
        self.holding(name, Holder::Shared(value))
    }

    pub fn shared_of<T: Any + Send + Sync>(&self, value: Value) -> Option<Arc<T>> {
        match &self.foreign_entry(value)?.holder {
            Holder::Shared(shared) => Arc::clone(shared).downcast::<T>().ok(),
            Holder::Local(_) => None,
        }
    }

    pub fn foreign_of<T: Any>(&self, value: Value) -> Option<&T> {
        self.foreign_entry(value)?.get()
    }

    pub(crate) fn foreign_entry(&self, value: Value) -> Option<&Foreign> {
        self.foreigns.get(reference(value, Kind::Foreign)?)
    }

    pub(crate) fn address_of(&self, value: Value) -> Option<usize> {
        if let Some(channel) = self.channel_of(value) {
            return Some(Arc::as_ptr(channel).addr());
        }
        self.foreign_entry(value).map(Foreign::address)
    }

    pub(crate) fn set_promise(&mut self, value: Value, promise: Promise) -> bool {
        let Some(slot) =
            reference(value, Kind::Promise).and_then(|index| self.promises.get_mut(index))
        else {
            return false;
        };
        *slot = promise;
        true
    }

    pub fn list(
        &mut self,
        items: impl DoubleEndedIterator<Item = Value>,
    ) -> Result<Value, HeapFull> {
        items
            .rev()
            .try_fold(Value::Nil, |tail, head| self.pair(head, tail))
    }

    pub fn map_from(&mut self, items: &[Value]) -> Result<Value, crate::error::Error> {
        crate::vm::build::make_map(self, items)
    }

    pub fn vector_from(&mut self, items: &[Value]) -> Result<Value, HeapFull> {
        let vector = Vector::from_items(self, items)?;
        self.vector(vector)
    }

    pub fn elements(&self, value: Value) -> Option<Vec<Value>> {
        if value == Value::Nil || self.pair_of(value).is_some() {
            return Some(self.items(value));
        }
        if let Some(vector) = self.vector_of(value) {
            return Some(vector.items(self));
        }
        let set = self.set_of(value)?;
        Some(
            set.entries(self)
                .into_iter()
                .map(|(item, _)| item)
                .collect(),
        )
    }

    pub fn entries(&self, value: Value) -> Option<Vec<(Value, Value)>> {
        Some(self.map_of(value)?.entries(self))
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
