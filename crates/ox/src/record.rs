use crate::heap::Heap;
use crate::symbol::Sym;
use crate::value::Value;

#[derive(Debug, Clone)]
pub struct RecordType {
    pub name: Sym,
    pub fields: Box<[Sym]>,
    pub constructor: Value,
}

#[derive(Debug, Clone)]
pub struct Record {
    pub kind: Value,
    pub fields: Box<[Value]>,
}

impl Record {
    pub fn slot(&self, heap: &Heap, name: Sym) -> Option<usize> {
        heap.record_type_of(self.kind)?
            .fields
            .iter()
            .position(|field| *field == name)
    }

    pub fn get(&self, heap: &Heap, name: Sym) -> Option<Value> {
        self.fields.get(self.slot(heap, name)?).copied()
    }

    pub fn type_name(&self, heap: &Heap) -> &'static str {
        heap.record_type_of(self.kind)
            .map_or("record", |kind| kind.name.name())
    }
}
