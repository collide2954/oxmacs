use std::collections::HashSet;
use std::rc::{Rc, Weak};

use super::Heap;
use crate::bytecode::Code;
use crate::value::promise::Promise;
use crate::value::{Kind, Value};
use crate::vm::machine::Generator;

const MINIMUM: usize = 100_000;

impl Heap {
    pub fn in_use(&self) -> usize {
        self.strings.in_use()
            + self.pairs.in_use()
            + self.natives.in_use()
            + self.closures.in_use()
            + self.cells.in_use()
            + self.bigints.in_use()
            + self.vectors.in_use()
            + self.nodes.in_use()
            + self.maps.in_use()
            + self.map_nodes.in_use()
            + self.sets.in_use()
            + self.record_types.in_use()
            + self.records.in_use()
            + self.atoms.in_use()
            + self.identifiers.in_use()
            + self.generators.in_use()
            + self.promises.in_use()
            + self.channels.in_use()
            + self.foreigns.in_use()
    }

    pub fn wants_collection(&self) -> bool {
        self.in_use() >= self.next_collection.max(MINIMUM)
    }

    pub fn collect(&mut self, roots: &[Value], codes: &[Rc<Code>]) -> usize {
        self.unmark();
        let mut gray = roots.to_vec();
        gray.extend(self.rooted());
        let mut codes = codes.to_vec();
        let mut seen = HashSet::new();
        loop {
            while let Some(value) = gray.pop() {
                self.trace(value, &mut gray, &mut codes);
            }
            let Some(code) = codes.pop() else {
                break;
            };
            if seen.insert(Rc::as_ptr(&code)) {
                gray.extend(code.constants.iter().copied());
                codes.extend(code.functions.iter().cloned());
            }
        }
        let freed = self.sweep();
        self.next_collection = self.in_use() * 2;
        freed
    }

    fn rooted(&mut self) -> Vec<Value> {
        self.roots.retain(|root| root.strong_count() > 0);
        self.roots
            .iter()
            .filter_map(Weak::upgrade)
            .map(|cell| cell.get())
            .collect()
    }

    fn unmark(&mut self) {
        self.strings.unmark();
        self.pairs.unmark();
        self.natives.unmark();
        self.closures.unmark();
        self.cells.unmark();
        self.bigints.unmark();
        self.vectors.unmark();
        self.nodes.unmark();
        self.maps.unmark();
        self.map_nodes.unmark();
        self.sets.unmark();
        self.record_types.unmark();
        self.records.unmark();
        self.atoms.unmark();
        self.identifiers.unmark();
        self.generators.unmark();
        self.promises.unmark();
        self.channels.unmark();
        self.foreigns.unmark();
    }

    fn sweep(&mut self) -> usize {
        self.strings.sweep()
            + self.pairs.sweep()
            + self.natives.sweep()
            + self.closures.sweep()
            + self.cells.sweep()
            + self.bigints.sweep()
            + self.vectors.sweep()
            + self.nodes.sweep()
            + self.maps.sweep()
            + self.map_nodes.sweep()
            + self.sets.sweep()
            + self.record_types.sweep()
            + self.records.sweep()
            + self.atoms.sweep()
            + self.identifiers.sweep()
            + self.generators.sweep()
            + self.promises.sweep()
            + self.channels.sweep()
            + self.foreigns.sweep()
    }

    fn trace(&mut self, value: Value, gray: &mut Vec<Value>, codes: &mut Vec<Rc<Code>>) {
        let Value::Ref(reference) = value else {
            return;
        };
        let index = reference.index;
        match reference.kind {
            Kind::String => {
                self.strings.mark(index);
            }
            Kind::BigInt => {
                self.bigints.mark(index);
            }
            Kind::Native => {
                self.natives.mark(index);
            }
            Kind::Identifier => {
                self.identifiers.mark(index);
            }
            Kind::Pair => {
                if self.pairs.mark(index)
                    && let Some(pair) = self.pairs.get(index)
                {
                    gray.extend([pair.head, pair.tail]);
                }
            }
            Kind::Closure => {
                if self.closures.mark(index)
                    && let Some(closure) = self.closures.get(index)
                {
                    gray.extend(closure.captured.iter().copied());
                    codes.push(Rc::clone(&closure.code));
                }
            }
            Kind::Cell => {
                if self.cells.mark(index)
                    && let Some(held) = self.cells.get(index)
                {
                    gray.push(*held);
                }
            }
            Kind::Atom => {
                if self.atoms.mark(index)
                    && let Some(held) = self.atoms.get(index)
                {
                    gray.push(*held);
                }
            }
            Kind::Vector => {
                if self.vectors.mark(index)
                    && let Some(vector) = self.vectors.get(index)
                {
                    gray.extend(vector.children());
                }
            }
            Kind::VecNode => {
                if self.nodes.mark(index)
                    && let Some(items) = self.nodes.get(index)
                {
                    gray.extend(items.iter().copied());
                }
            }
            Kind::Map => {
                if self.maps.mark(index)
                    && let Some(map) = self.maps.get(index)
                {
                    gray.extend(map.children());
                }
            }
            Kind::Set => {
                if self.sets.mark(index)
                    && let Some(set) = self.sets.get(index)
                {
                    gray.extend(set.children());
                }
            }
            Kind::MapNode => {
                if self.map_nodes.mark(index)
                    && let Some(node) = self.map_nodes.get(index)
                {
                    node.children(gray);
                }
            }
            Kind::RecordType => {
                if self.record_types.mark(index)
                    && let Some(kind) = self.record_types.get(index)
                {
                    gray.push(kind.constructor);
                }
            }
            Kind::Record => {
                if self.records.mark(index)
                    && let Some(record) = self.records.get(index)
                {
                    gray.push(record.kind);
                    gray.extend(record.fields.iter().copied());
                }
            }
            Kind::Generator | Kind::Promise | Kind::Channel | Kind::Foreign => {
                self.trace_task(reference.kind, index, gray, codes);
            }
        }
    }

    fn trace_task(
        &mut self,
        kind: Kind,
        index: u32,
        gray: &mut Vec<Value>,
        codes: &mut Vec<Rc<Code>>,
    ) {
        match kind {
            Kind::Generator => {
                if self.generators.mark(index)
                    && let Some(Generator::Ready(machine, _)) = self.generators.get(index)
                {
                    machine.roots(gray, codes);
                }
            }
            Kind::Promise => {
                if self.promises.mark(index)
                    && let Some(Promise::Delivered(held) | Promise::Failed(held)) =
                        self.promises.get(index)
                {
                    gray.push(*held);
                }
            }
            Kind::Channel => {
                self.channels.mark(index);
            }
            _ => {
                self.foreigns.mark(index);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::heap::Heap;
    use crate::value::Value;

    #[test]
    fn unreachable_objects_are_freed_and_reachable_ones_kept() {
        let mut heap = Heap::default();
        let kept = heap.string("kept").unwrap();
        let garbage = heap.string("garbage").unwrap();
        let list = heap.list([kept, Value::Int(1)].into_iter()).unwrap();

        let freed = heap.collect(&[list], &[]);

        assert_eq!(freed, 1);
        assert_eq!(heap.text(kept), Some("kept"));
        assert_eq!(heap.text(garbage), None);
        assert_eq!(heap.items(list), [kept, Value::Int(1)]);
    }

    #[test]
    fn freed_slots_are_reused() {
        let mut heap = Heap::default();
        heap.string("lost").unwrap();
        heap.collect(&[], &[]);

        let before = heap.in_use();
        let again = heap.string("again").unwrap();

        assert_eq!(heap.in_use(), before + 1);
        assert_eq!(heap.text(again), Some("again"));
    }
}
