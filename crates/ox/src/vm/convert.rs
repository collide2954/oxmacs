use std::sync::Arc;

use super::Runtime;
use crate::error::Error;
use crate::portable::{Channel, Portable};
use crate::symbol::Sym;
use crate::value::Value;
use crate::value::foreign::Holder;
use crate::value::print;
use crate::value::record::{Family, Record, RecordType};
use crate::vm::build::{full, make_map, make_set, make_vector};

impl Runtime {
    pub(crate) fn portable(&self, value: Value, purpose: &str) -> Result<Portable, Error> {
        let heap = &self.heap;
        let refused = || {
            Error::new(format!(
                "{} can't be sent {purpose}",
                print::write(heap, value)
            ))
        };
        let all = |items: Vec<Value>| -> Result<Vec<Portable>, Error> {
            items
                .into_iter()
                .map(|item| self.portable(item, purpose))
                .collect()
        };
        Ok(match value {
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
                    Portable::List(all(items)?, Box::new(self.portable(rest, purpose)?))
                } else if let Some(vector) = heap.vector_of(value) {
                    Portable::Vector(all(vector.items(heap))?)
                } else if let Some(map) = heap.map_of(value) {
                    let mut entries = Vec::with_capacity(map.count);
                    for (key, item) in map.entries(heap) {
                        entries.push((self.portable(key, purpose)?, self.portable(item, purpose)?));
                    }
                    Portable::Map(entries)
                } else if let Some(set) = heap.set_of(value) {
                    Portable::Set(all(set
                        .entries(heap)
                        .into_iter()
                        .map(|(item, _)| item)
                        .collect())?)
                } else if let Some(record) = heap.record_of(value) {
                    let kind = heap.record_type_of(record.kind).ok_or_else(refused)?;
                    Portable::Record {
                        module: kind.module,
                        name: kind.name,
                        family: kind.family,
                        fields: kind.fields.clone(),
                        values: all(record.fields.to_vec())?,
                    }
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
        })
    }

    pub(crate) fn rebuild(&mut self, portable: &Portable) -> Result<Value, Error> {
        Ok(match portable {
            Portable::Nil => Value::Nil,
            Portable::Bool(flag) => Value::Bool(*flag),
            Portable::Int(number) => Value::Int(*number),
            Portable::Float(number) => Value::Float(*number),
            Portable::Char(character) => Value::Char(*character),
            Portable::Symbol(name) => Value::Symbol(*name),
            Portable::Keyword(name) => Value::Keyword(*name),
            Portable::String(text) => self.heap.string(text).map_err(full)?,
            Portable::BigInt(number) => self.heap.bigint(number.clone()).map_err(full)?,
            Portable::List(items, tail) => {
                let mut list = self.rebuild(tail)?;
                for item in items.iter().rev() {
                    let head = self.rebuild(item)?;
                    list = self.heap.pair(head, list).map_err(full)?;
                }
                list
            }
            Portable::Vector(items) => {
                let items = self.rebuild_all(items)?;
                make_vector(&mut self.heap, &items)?
            }
            Portable::Map(entries) => {
                let mut items = Vec::with_capacity(entries.len() * 2);
                for (key, item) in entries {
                    items.push(self.rebuild(key)?);
                    items.push(self.rebuild(item)?);
                }
                make_map(&mut self.heap, &items)?
            }
            Portable::Set(items) => {
                let items = self.rebuild_all(items)?;
                make_set(&mut self.heap, &items)?
            }
            Portable::Record {
                module,
                name,
                family,
                fields,
                values,
            } => {
                let kind = self.record_type_for(*module, *name, *family, fields)?;
                let values = self.rebuild_all(values)?;
                self.heap
                    .record(Record {
                        kind,
                        fields: values.into(),
                    })
                    .map_err(full)?
            }
            Portable::Channel(channel) => self.intern_channel(channel)?,
            Portable::Foreign(name, shared) => self
                .heap
                .shared_foreign(*name, Arc::clone(shared))
                .map_err(full)?,
        })
    }

    fn rebuild_all(&mut self, items: &[Portable]) -> Result<Vec<Value>, Error> {
        items.iter().map(|item| self.rebuild(item)).collect()
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
        let kind = self
            .heap
            .record_type(RecordType {
                name,
                fields: fields.into(),
                constructor: Value::Nil,
                family,
                module,
            })
            .map_err(full)?;
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
        let value = self.heap.channel(Arc::clone(channel)).map_err(full)?;
        self.channels.insert(address, value);
        Ok(value)
    }
}
