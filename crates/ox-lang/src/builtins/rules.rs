use std::collections::HashMap;

use crate::error::Error;
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::symbol::Sym;
use crate::value::Value;
use crate::value::equal::equal;
use crate::value::map::Map;
use crate::value::print;
use crate::value::vector::Vector;

#[derive(Debug, Clone)]
enum Bound {
    One(Value),
    Many(Vec<Bound>),
}

type Bindings = HashMap<Sym, Bound>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    List,
    Vector,
}

fn name_of(heap: &Heap, value: Value) -> Option<Sym> {
    match value {
        Value::Symbol(name) => Some(name),
        _ => heap.identifier_of(value).map(|identifier| identifier.name),
    }
}

fn sequence(heap: &Heap, value: Value) -> Option<(Shape, Vec<Value>)> {
    if let Some(vector) = heap.vector_of(value) {
        return Some((Shape::Vector, vector.items(heap)));
    }
    if value != Value::Nil && heap.pair_of(value).is_none() {
        return None;
    }
    let mut rest = value;
    while let Some(pair) = heap.pair_of(rest) {
        rest = pair.tail;
    }
    (rest == Value::Nil).then(|| (Shape::List, heap.items(value)))
}

struct Rules {
    literals: Vec<Sym>,
    ellipsis: Sym,
    underscore: Sym,
}

impl Rules {
    fn variables(&self, heap: &Heap, pattern: Value) -> Vec<Sym> {
        if let Some(name) = name_of(heap, pattern) {
            if name == self.underscore || name == self.ellipsis || self.literals.contains(&name) {
                return Vec::new();
            }
            return vec![name];
        }
        sequence(heap, pattern)
            .map(|(_, items)| {
                items
                    .into_iter()
                    .flat_map(|item| self.variables(heap, item))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn matches(&self, heap: &Heap, pattern: Value, input: Value, out: &mut Bindings) -> bool {
        if let Some(name) = name_of(heap, pattern) {
            if name == self.underscore {
                return true;
            }
            if self.literals.contains(&name) {
                return name_of(heap, input) == Some(name);
            }
            out.insert(name, Bound::One(input));
            return true;
        }
        if let Some((shape, patterns)) = sequence(heap, pattern) {
            return match sequence(heap, input) {
                Some((input_shape, inputs)) if input_shape == shape => {
                    self.all(heap, &patterns, &inputs, out)
                }
                _ => false,
            };
        }
        equal(heap, pattern, input)
    }

    fn all(&self, heap: &Heap, patterns: &[Value], inputs: &[Value], out: &mut Bindings) -> bool {
        let dots = patterns
            .iter()
            .position(|pattern| name_of(heap, *pattern) == Some(self.ellipsis));
        let Some(dots) = dots else {
            return patterns.len() == inputs.len()
                && patterns
                    .iter()
                    .zip(inputs)
                    .all(|(pattern, input)| self.matches(heap, *pattern, *input, out));
        };
        let Some(repeated) = dots.checked_sub(1) else {
            return false;
        };
        let (before, after) = (&patterns[..repeated], &patterns[dots + 1..]);
        let Some(count) = inputs.len().checked_sub(before.len() + after.len()) else {
            return false;
        };
        if !before
            .iter()
            .zip(inputs)
            .all(|(pattern, input)| self.matches(heap, *pattern, *input, out))
        {
            return false;
        }
        let mut repetitions = Vec::with_capacity(count);
        for input in &inputs[before.len()..before.len() + count] {
            let mut each = Bindings::new();
            if !self.matches(heap, patterns[repeated], *input, &mut each) {
                return false;
            }
            repetitions.push(each);
        }
        for name in self.variables(heap, patterns[repeated]) {
            let values = repetitions
                .iter()
                .map(|each| each.get(&name).cloned().unwrap_or(Bound::Many(Vec::new())))
                .collect();
            out.insert(name, Bound::Many(values));
        }
        after
            .iter()
            .zip(&inputs[before.len() + count..])
            .all(|(pattern, input)| self.matches(heap, *pattern, *input, out))
    }

    fn substitute(
        &self,
        heap: &mut Heap,
        template: Value,
        bindings: &Bindings,
    ) -> Result<Value, Error> {
        if let Some(name) = name_of(heap, template) {
            return match bindings.get(&name) {
                Some(Bound::One(value)) => Ok(*value),
                Some(Bound::Many(_)) => Err(Error::new(format!(
                    "{} needs ... after it in the template",
                    name.name()
                ))),
                None => Ok(template),
            };
        }
        if let Some(map) = heap.map_of(template) {
            return self.substitute_map(heap, map, bindings);
        }
        let Some((shape, items)) = sequence(heap, template) else {
            return Ok(template);
        };
        let mut out = Vec::with_capacity(items.len());
        let mut index = 0;
        while index < items.len() {
            let item = items[index];
            let repeated = items
                .get(index + 1)
                .is_some_and(|next| name_of(heap, *next) == Some(self.ellipsis));
            if repeated {
                out.extend(self.repeat(heap, item, bindings)?);
                index += 2;
            } else {
                out.push(self.substitute(heap, item, bindings)?);
                index += 1;
            }
        }
        match shape {
            Shape::List => heap.list(out.into_iter()).map_err(Error::from),
            Shape::Vector => {
                let vector = Vector::from_items(heap, &out)?;
                heap.vector(vector).map_err(Error::from)
            }
        }
    }

    fn substitute_map(
        &self,
        heap: &mut Heap,
        map: Map,
        bindings: &Bindings,
    ) -> Result<Value, Error> {
        let mut result = Map::EMPTY;
        for (key, value) in map.entries(heap) {
            let key = self.substitute(heap, key, bindings)?;
            let value = self.substitute(heap, value, bindings)?;
            result = result.assoc(heap, key, value)?;
        }
        heap.map(result).map_err(Error::from)
    }

    fn repeat(
        &self,
        heap: &mut Heap,
        item: Value,
        bindings: &Bindings,
    ) -> Result<Vec<Value>, Error> {
        let repeated: Vec<(Sym, Vec<Bound>)> = self
            .variables(heap, item)
            .into_iter()
            .filter_map(|name| match bindings.get(&name) {
                Some(Bound::Many(values)) => Some((name, values.clone())),
                _ => None,
            })
            .collect();
        let Some(count) = repeated.first().map(|(_, values)| values.len()) else {
            return Err(Error::new(
                "... follows a part of the template with no repeated pattern names",
            ));
        };
        if repeated.iter().any(|(_, values)| values.len() != count) {
            return Err(Error::new(
                "Names repeated with ... matched different numbers of forms",
            ));
        }
        let mut out = Vec::with_capacity(count);
        for index in 0..count {
            let mut each = bindings.clone();
            for (name, values) in &repeated {
                if let Some(value) = values.get(index) {
                    each.insert(*name, value.clone());
                }
            }
            out.push(self.substitute(heap, item, &each)?);
        }
        Ok(out)
    }
}

fn expand_rules(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let [name, literals, rules, form] = arguments else {
        return Err(Error::new(
            "expand-rules takes a name, literals, rules and a form",
        ));
    };
    let literals = sequence(heap, *literals)
        .map(|(_, items)| {
            items
                .into_iter()
                .filter_map(|item| name_of(heap, item))
                .collect()
        })
        .unwrap_or_default();
    let matcher = Rules {
        literals,
        ellipsis: Sym::intern("..."),
        underscore: Sym::intern("_"),
    };
    let inputs = sequence(heap, *form)
        .map(|(_, items)| items)
        .unwrap_or_default();
    let rules = sequence(heap, *rules)
        .map(|(_, items)| items)
        .unwrap_or_default();
    for rule in rules {
        let Some((_, parts)) = sequence(heap, rule) else {
            continue;
        };
        let [pattern, template] = parts.as_slice() else {
            continue;
        };
        let Some((_, pattern)) = sequence(heap, *pattern) else {
            continue;
        };
        let mut bindings = Bindings::new();
        if matcher.all(
            heap,
            pattern.get(1..).unwrap_or_default(),
            &inputs,
            &mut bindings,
        ) {
            return matcher.substitute(heap, *template, &bindings);
        }
    }
    let mut call = vec![print::brief(heap, *name)];
    call.extend(inputs.iter().map(|input| print::brief(heap, *input)));
    Err(Error::new(format!(
        "No rule of {} matches ({})",
        print::brief(heap, *name),
        call.join(" ")
    )))
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "expand-rules")
        .arity(4, Some(4))
        .function(expand_rules);
}
