use std::sync::Arc;

use super::Runtime;
use crate::error::Error;
use crate::portable::{Channel, Portable};
use crate::symbol::Sym;
use crate::value::Value;
use crate::value::foreign::Holder;
use crate::value::print;
use crate::value::record::{Family, Record, RecordType};
use crate::vm::build::{make_map, make_set, make_vector};

pub(crate) const DEEPEST: usize = 256;

enum Gather {
    Visit(Value, usize),
    List(usize),
    Vector(usize),
    Map(usize),
    Set(usize),
    Record {
        module: Sym,
        name: Sym,
        family: Family,
        fields: Box<[Sym]>,
        count: usize,
    },
}

enum Spread<'a> {
    Visit(&'a Portable),
    List(usize),
    Vector(usize),
    Map(usize),
    Set(usize),
    Record(&'a Portable),
}

fn last(done: &mut Vec<Portable>, count: usize) -> Vec<Portable> {
    done.split_off(done.len().saturating_sub(count))
}

impl Runtime {
    pub(crate) fn portable(&self, value: Value, purpose: &str) -> Result<Portable, Error> {
        let mut tasks = vec![Gather::Visit(value, 0)];
        let mut done = Vec::new();
        while let Some(task) = tasks.pop() {
            match task {
                Gather::Visit(value, depth) => {
                    if let Some(portable) = self.gather(value, purpose, depth, &mut tasks)? {
                        done.push(portable);
                    }
                }
                Gather::List(count) => {
                    let tail = done.pop().unwrap_or(Portable::Nil);
                    let items = last(&mut done, count);
                    done.push(Portable::List(items, Box::new(tail)));
                }
                Gather::Vector(count) => {
                    let items = last(&mut done, count);
                    done.push(Portable::Vector(items));
                }
                Gather::Map(count) => {
                    let mut flat = last(&mut done, count * 2).into_iter();
                    let mut entries = Vec::with_capacity(count);
                    while let (Some(key), Some(item)) = (flat.next(), flat.next()) {
                        entries.push((key, item));
                    }
                    done.push(Portable::Map(entries));
                }
                Gather::Set(count) => {
                    let items = last(&mut done, count);
                    done.push(Portable::Set(items));
                }
                Gather::Record {
                    module,
                    name,
                    family,
                    fields,
                    count,
                } => {
                    let values = last(&mut done, count);
                    done.push(Portable::Record {
                        module,
                        name,
                        family,
                        fields,
                        values,
                    });
                }
            }
        }
        Ok(done.pop().unwrap_or(Portable::Nil))
    }

    fn gather(
        &self,
        value: Value,
        purpose: &str,
        depth: usize,
        tasks: &mut Vec<Gather>,
    ) -> Result<Option<Portable>, Error> {
        let heap = &self.heap;
        let refused = || {
            Error::new(format!(
                "{} can't be sent {purpose}",
                print::brief(heap, value)
            ))
        };
        let mut children =
            |assemble: Gather, items: Vec<Value>| -> Result<Option<Portable>, Error> {
                if depth >= DEEPEST {
                    return Err(Error::new(format!(
                        "{} is nested more than {DEEPEST} levels deep to be sent {purpose}",
                        print::brief(heap, value)
                    )));
                }
                tasks.push(assemble);
                tasks.extend(
                    items
                        .into_iter()
                        .rev()
                        .map(|item| Gather::Visit(item, depth + 1)),
                );
                Ok(None)
            };
        Ok(Some(match value {
            Value::Nil => Portable::Nil,
            Value::Bool(flag) => Portable::Bool(flag),
            Value::Int(number) => Portable::Int(number),
            Value::Float(number) => Portable::Float(number),
            Value::Char(character) => Portable::Char(character),
            Value::Symbol(name) => Portable::Symbol(name),
            Value::Keyword(name) => Portable::Keyword(name),
            Value::Ref(_) => {
                if let Some(text) = heap.text(value) {
                    Portable::String(text.into())
                } else if let Some(number) = heap.bigint_of(value) {
                    Portable::BigInt(number.clone())
                } else if heap.pair_of(value).is_some() {
                    let mut items = Vec::new();
                    let mut rest = value;
                    while let Some(pair) = heap.pair_of(rest) {
                        items.push(pair.head);
                        rest = pair.tail;
                    }
                    let count = items.len();
                    items.push(rest);
                    return children(Gather::List(count), items);
                } else if let Some(vector) = heap.vector_of(value) {
                    let items = vector.items(heap);
                    return children(Gather::Vector(items.len()), items);
                } else if let Some(map) = heap.map_of(value) {
                    let items: Vec<Value> = map
                        .entries(heap)
                        .into_iter()
                        .flat_map(|(key, item)| [key, item])
                        .collect();
                    return children(Gather::Map(items.len() / 2), items);
                } else if let Some(set) = heap.set_of(value) {
                    let items: Vec<Value> = set
                        .entries(heap)
                        .into_iter()
                        .map(|(item, _)| item)
                        .collect();
                    return children(Gather::Set(items.len()), items);
                } else if let Some(record) = heap.record_of(value) {
                    let kind = heap.record_type_of(record.kind).ok_or_else(refused)?;
                    let items = record.fields.to_vec();
                    return children(
                        Gather::Record {
                            module: kind.module,
                            name: kind.name,
                            family: kind.family,
                            fields: kind.fields.clone(),
                            count: items.len(),
                        },
                        items,
                    );
                } else if let Some(identifier) = heap.identifier_of(value) {
                    Portable::Symbol(identifier.name)
                } else if let Some(channel) = heap.channel_of(value) {
                    Portable::Channel(Arc::clone(channel))
                } else if let Some(foreign) = heap.foreign_entry(value)
                    && let Holder::Shared(shared) = &foreign.holder
                {
                    Portable::Foreign(foreign.name, Arc::clone(shared))
                } else {
                    return Err(refused());
                }
            }
        }))
    }

    pub(crate) fn rebuild(&mut self, portable: &Portable) -> Result<Value, Error> {
        let mut tasks = vec![Spread::Visit(portable)];
        let mut done: Vec<Value> = Vec::new();
        while let Some(task) = tasks.pop() {
            match task {
                Spread::Visit(portable) => {
                    if let Some(value) = self.unpack(portable, &mut tasks)? {
                        done.push(value);
                    }
                }
                Spread::List(count) => {
                    let mut list = done.pop().unwrap_or(Value::Nil);
                    let items = done.split_off(done.len().saturating_sub(count));
                    for head in items.into_iter().rev() {
                        list = self.heap.pair(head, list)?;
                    }
                    done.push(list);
                }
                Spread::Vector(count) => {
                    let items = done.split_off(done.len().saturating_sub(count));
                    done.push(make_vector(&mut self.heap, &items)?);
                }
                Spread::Map(count) => {
                    let items = done.split_off(done.len().saturating_sub(count * 2));
                    done.push(make_map(&mut self.heap, &items)?);
                }
                Spread::Set(count) => {
                    let items = done.split_off(done.len().saturating_sub(count));
                    done.push(make_set(&mut self.heap, &items)?);
                }
                Spread::Record(Portable::Record {
                    module,
                    name,
                    family,
                    fields,
                    values,
                }) => {
                    let kind = self.record_type_for(*module, *name, *family, fields)?;
                    let values = done.split_off(done.len().saturating_sub(values.len()));
                    done.push(self.heap.record(Record {
                        kind,
                        fields: values.into(),
                    })?);
                }
                Spread::Record(_) => {}
            }
        }
        Ok(done.pop().unwrap_or(Value::Nil))
    }

    fn unpack<'a>(
        &mut self,
        portable: &'a Portable,
        tasks: &mut Vec<Spread<'a>>,
    ) -> Result<Option<Value>, Error> {
        Ok(Some(match portable {
            Portable::Nil => Value::Nil,
            Portable::Bool(flag) => Value::Bool(*flag),
            Portable::Int(number) => Value::Int(*number),
            Portable::Float(number) => Value::Float(*number),
            Portable::Char(character) => Value::Char(*character),
            Portable::Symbol(name) => Value::Symbol(*name),
            Portable::Keyword(name) => Value::Keyword(*name),
            Portable::String(text) => self.heap.string(text)?,
            Portable::BigInt(number) => self.heap.bigint(number.clone())?,
            Portable::List(items, tail) => {
                tasks.push(Spread::List(items.len()));
                tasks.push(Spread::Visit(tail));
                tasks.extend(items.iter().rev().map(Spread::Visit));
                return Ok(None);
            }
            Portable::Vector(items) => {
                tasks.push(Spread::Vector(items.len()));
                tasks.extend(items.iter().rev().map(Spread::Visit));
                return Ok(None);
            }
            Portable::Map(entries) => {
                tasks.push(Spread::Map(entries.len()));
                for (key, item) in entries.iter().rev() {
                    tasks.push(Spread::Visit(item));
                    tasks.push(Spread::Visit(key));
                }
                return Ok(None);
            }
            Portable::Set(items) => {
                tasks.push(Spread::Set(items.len()));
                tasks.extend(items.iter().rev().map(Spread::Visit));
                return Ok(None);
            }
            Portable::Record { values, .. } => {
                tasks.push(Spread::Record(portable));
                tasks.extend(values.iter().rev().map(Spread::Visit));
                return Ok(None);
            }
            Portable::Channel(channel) => self.intern_channel(channel)?,
            Portable::Foreign(name, shared) => {
                self.heap.shared_foreign(*name, Arc::clone(shared))?
            }
        }))
    }
    fn record_type_for(
        &mut self,
        module: Sym,
        name: Sym,
        family: Family,
        fields: &[Sym],
    ) -> Result<Value, Error> {
        let known = self
            .module_ids
            .get(module.name())
            .and_then(|id| self.modules.get(*id))
            .and_then(|found| found.globals.get(&name))
            .copied()
            .filter(|kind| {
                self.heap
                    .record_type_of(*kind)
                    .is_some_and(|found| found.module == module && *found.fields == *fields)
            });
        if let Some(kind) = known {
            return Ok(kind);
        }
        if let Some(kind) = self.stand_ins.get(&(module, name)) {
            return Ok(*kind);
        }
        let kind = self.heap.record_type(RecordType {
            name,
            fields: fields.into(),
            constructor: Value::Nil,
            family,
            module,
        })?;
        self.stand_ins.insert((module, name), kind);
        Ok(kind)
    }

    fn intern_channel(&mut self, channel: &Arc<Channel>) -> Result<Value, Error> {
        let address = Arc::as_ptr(channel).addr();
        if let Some(value) = self.channels.get(&address).copied()
            && self
                .heap
                .channel_of(value)
                .is_some_and(|found| Arc::ptr_eq(found, channel))
        {
            return Ok(value);
        }
        let value = self.heap.channel(Arc::clone(channel))?;
        self.channels.insert(address, value);
        Ok(value)
    }
}
